// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::VecDeque;
use std::future::Future;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::atomic::Ordering::Acquire;
use std::sync::Arc;

use common_base::tokio::task::JoinHandle;
use common_base::ProgressCallback;
use common_base::ProgressValues;
use common_base::Runtime;
use common_base::TrySpawn;
use common_dal::AzureBlobAccessor;
use common_dal::DalMetrics;
use common_dal::DataAccessor;
use common_dal::DataAccessorInterceptor;
use common_dal::Local;
use common_dal::StorageScheme;
use common_dal::S3;
use common_exception::ErrorCode;
use common_exception::Result;
use common_infallible::RwLock;
use common_planners::Part;
use common_planners::Partitions;
use common_planners::PlanNode;
use common_planners::ReadDataSourcePlan;
use common_planners::Statistics;
use common_streams::AbortStream;
use common_streams::SendableDataBlockStream;

use crate::catalogs::impls::DatabaseCatalog;
use crate::catalogs::Catalog;
use crate::catalogs::Table;
use crate::clusters::Cluster;
use crate::configs::AzureStorageBlobConfig;
use crate::configs::Config;
use crate::servers::http::v1::query::HttpQueryHandle;
use crate::sessions::QueryContextShared;
use crate::sessions::SessionManager;
use crate::sessions::Settings;

pub struct QueryContext {
    version: String,
    statistics: Arc<RwLock<Statistics>>,
    partition_queue: Arc<RwLock<VecDeque<Part>>>,
    shared: Arc<QueryContextShared>,
}

impl QueryContext {
    pub fn new(other: Arc<QueryContext>) -> Arc<QueryContext> {
        QueryContext::from_shared(other.shared.clone())
    }

    pub fn from_shared(shared: Arc<QueryContextShared>) -> Arc<QueryContext> {
        shared.increment_ref_count();

        log::info!("Create DatabendQueryContext");

        Arc::new(QueryContext {
            statistics: Arc::new(RwLock::new(Statistics::default())),
            partition_queue: Arc::new(RwLock::new(VecDeque::new())),
            version: format!(
                "DatabendQuery v-{}",
                *crate::configs::DATABEND_COMMIT_VERSION
            ),
            shared,
        })
    }

    /// Build a table instance the plan wants to operate on.
    ///
    /// A plan just contains raw information about a table or table function.
    /// This method builds a `dyn Table`, which provides table specific io methods the plan needs.
    pub fn build_table_from_source_plan(
        &self,
        plan: &ReadDataSourcePlan,
    ) -> Result<Arc<dyn Table>> {
        let catalog = self.get_catalog();

        if plan.tbl_args.is_none() {
            catalog.build_table(&plan.table_info)
        } else {
            Ok(catalog
                .get_table_function(&plan.table_info.name, plan.tbl_args.clone())?
                .as_table())
        }
    }

    /// Set progress callback to context.
    /// By default, it is called for leaf sources, after each block
    /// Note that the callback can be called from different threads.
    pub fn progress_callback(&self) -> Result<ProgressCallback> {
        let current_progress = self.shared.progress.clone();
        Ok(Box::new(move |value: &ProgressValues| {
            current_progress.incr(value);
        }))
    }

    pub fn get_progress_value(&self) -> ProgressValues {
        self.shared.progress.as_ref().get_values()
    }

    pub fn get_and_reset_progress_value(&self) -> ProgressValues {
        self.shared.progress.as_ref().get_and_reset()
    }

    // Some table can estimate the approx total rows, such as NumbersTable
    pub fn add_total_rows_approx(&self, total_rows: usize) {
        self.shared
            .progress
            .as_ref()
            .add_total_rows_approx(total_rows);
    }

    // Steal n partitions from the partition pool by the pipeline worker.
    // This also can steal the partitions from distributed node.
    pub fn try_get_partitions(&self, num: usize) -> Result<Partitions> {
        let mut partitions = vec![];
        for _ in 0..num {
            match self.partition_queue.write().pop_back() {
                None => break,
                Some(partition) => {
                    partitions.push(partition);
                }
            }
        }
        Ok(partitions)
    }

    // Update the context partition pool from the pipeline builder.
    pub fn try_set_partitions(&self, partitions: Partitions) -> Result<()> {
        for part in partitions {
            self.partition_queue.write().push_back(part);
        }
        Ok(())
    }

    pub fn try_get_statistics(&self) -> Result<Statistics> {
        let statistics = self.statistics.read();
        Ok((*statistics).clone())
    }

    pub fn try_set_statistics(&self, val: &Statistics) -> Result<()> {
        *self.statistics.write() = val.clone();
        Ok(())
    }

    pub fn attach_http_query(&self, handle: HttpQueryHandle) {
        self.shared.attach_http_query(handle);
    }

    pub fn attach_query_str(&self, query: &str) {
        self.shared.attach_query_str(query);
    }

    pub fn attach_query_plan(&self, query_plan: &PlanNode) {
        self.shared.attach_query_plan(query_plan);
    }

    pub fn get_cluster(&self) -> Arc<Cluster> {
        self.shared.get_cluster()
    }

    pub fn get_catalog(&self) -> Arc<DatabaseCatalog> {
        self.shared.get_catalog()
    }

    /// Fetch a Table by db and table name.
    ///
    /// It guaranteed to return a consistent result for multiple calls, in a same query.
    /// E.g.:
    /// ```sql
    /// SELECT * FROM (SELECT * FROM db.table_name) as subquery_1, (SELECT * FROM db.table_name) AS subquery_2
    /// ```
    pub async fn get_table(&self, database: &str, table: &str) -> Result<Arc<dyn Table>> {
        self.shared.get_table(database, table).await
    }

    pub fn get_id(&self) -> String {
        self.shared.init_query_id.as_ref().read().clone()
    }

    pub fn try_create_abortable(&self, input: SendableDataBlockStream) -> Result<AbortStream> {
        let (abort_handle, abort_stream) = AbortStream::try_create(input)?;
        self.shared.add_source_abort_handle(abort_handle);
        Ok(abort_stream)
    }

    pub fn get_current_database(&self) -> String {
        self.shared.get_current_database()
    }

    pub fn get_current_user(&self) -> Result<String> {
        self.shared.get_current_user()
    }

    pub async fn set_current_database(&self, new_database_name: String) -> Result<()> {
        let catalog = self.get_catalog();
        match catalog.get_database(&new_database_name).await {
            Ok(_) => self.shared.set_current_database(new_database_name),
            Err(_) => {
                return Err(ErrorCode::UnknownDatabase(format!(
                    "Cannot USE '{}', because the '{}' doesn't exist",
                    new_database_name, new_database_name
                )));
            }
        };

        Ok(())
    }

    pub fn get_fuse_version(&self) -> String {
        self.version.clone()
    }

    pub fn get_settings(&self) -> Arc<Settings> {
        self.shared.get_settings()
    }

    pub fn get_config(&self) -> Config {
        self.shared.conf.clone()
    }

    pub fn get_subquery_name(&self, _query: &PlanNode) -> String {
        let index = self.shared.subquery_index.fetch_add(1, Ordering::Relaxed);
        format!("_subquery_{}", index)
    }

    pub fn get_sessions_manager(self: &Arc<Self>) -> Arc<SessionManager> {
        self.shared.session.get_sessions_manager()
    }

    pub fn get_shared_runtime(&self) -> Result<Arc<Runtime>> {
        self.shared.try_get_runtime()
    }

    pub fn get_data_accessor(&self) -> Result<Arc<dyn DataAccessor>> {
        let storage_conf = &self.get_config().storage;
        let scheme_name = &storage_conf.storage_type;
        let scheme = StorageScheme::from_str(scheme_name)?;
        let da: Arc<dyn DataAccessor> = match scheme {
            StorageScheme::S3 => {
                let conf = &storage_conf.s3;
                Arc::new(S3::try_create(
                    &conf.region,
                    &conf.endpoint_url,
                    &conf.bucket,
                    &conf.access_key_id,
                    &conf.secret_access_key,
                )?)
            }
            StorageScheme::AzureStorageBlob => {
                let conf: &AzureStorageBlobConfig = &storage_conf.azure_storage_blob;
                Arc::new(AzureBlobAccessor::with_credentials(
                    &conf.account,
                    &conf.container,
                    &conf.master_key,
                ))
            }
            StorageScheme::LocalFs => Arc::new(Local::new(storage_conf.disk.data_path.as_str())),
        };

        Ok(Arc::new(DataAccessorInterceptor::new(
            self.shared.dal_ctx.clone(),
            da,
        )))
    }

    /// Get the data accessor metrics.
    pub fn get_dal_metrics(&self) -> DalMetrics {
        self.shared.dal_ctx.get_metrics()
    }
}

impl TrySpawn for QueryContext {
    /// Spawns a new asynchronous task, returning a tokio::JoinHandle for it.
    /// The task will run in the current context thread_pool not the global.
    fn try_spawn<T>(&self, task: T) -> Result<JoinHandle<T::Output>>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        Ok(self.shared.try_get_runtime()?.spawn(task))
    }
}

impl std::fmt::Debug for QueryContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.get_settings())
    }
}

impl Drop for QueryContext {
    fn drop(&mut self) {
        self.shared.destroy_context_ref()
    }
}

impl QueryContextShared {
    pub(in crate::sessions) fn destroy_context_ref(&self) {
        if self.ref_count.fetch_sub(1, Ordering::Release) == 1 {
            std::sync::atomic::fence(Acquire);
            log::info!("Destroy DatabendQueryContext");
            self.session.destroy_context_shared();
        }
    }

    pub(in crate::sessions) fn increment_ref_count(&self) {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
    }
}
