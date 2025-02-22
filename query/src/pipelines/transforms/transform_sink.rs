// Copyright 2020 Datafuse Labs.
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

use std::any::Any;
use std::sync::Arc;

use common_exception::Result;
use common_functions::scalars::CastFunction;
use common_meta_types::TableInfo;
use common_streams::CastStream;
use common_streams::SendableDataBlockStream;
use common_tracing::tracing;

use crate::catalogs::Catalog;
use crate::pipelines::processors::EmptyProcessor;
use crate::pipelines::processors::Processor;
use crate::sessions::QueryContext;

pub struct SinkTransform {
    ctx: Arc<QueryContext>,
    table_info: TableInfo,
    input: Arc<dyn Processor>,
    cast_needed: bool,
}

impl SinkTransform {
    pub fn create(ctx: Arc<QueryContext>, table_info: TableInfo, cast_needed: bool) -> Self {
        Self {
            ctx,
            table_info,
            input: Arc::new(EmptyProcessor::create()),
            cast_needed,
        }
    }
    fn table_info(&self) -> &TableInfo {
        &self.table_info
    }
}

#[async_trait::async_trait]
impl Processor for SinkTransform {
    fn name(&self) -> &str {
        "SinkTransform"
    }

    fn connect_to(&mut self, input: Arc<dyn Processor>) -> Result<()> {
        self.input = input;
        Ok(())
    }

    fn inputs(&self) -> Vec<Arc<dyn Processor>> {
        vec![self.input.clone()]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn execute(&self) -> Result<SendableDataBlockStream> {
        tracing::debug!("executing sink transform");
        let tbl = self.ctx.get_catalog().build_table(self.table_info())?;
        let mut upstream = self.input.execute().await?;
        let output_schema = self.table_info.schema();
        if self.cast_needed {
            let mut functions = Vec::with_capacity(output_schema.fields().len());
            for field in output_schema.fields() {
                let cast_function =
                    CastFunction::create("cast".to_string(), field.data_type().clone())?;
                functions.push(cast_function);
            }
            upstream = Box::pin(CastStream::try_create(upstream, output_schema, functions)?);
        };

        tbl.append_data(self.ctx.clone(), upstream).await
    }
}
