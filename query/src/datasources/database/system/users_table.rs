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

use std::any::Any;
use std::sync::Arc;

use common_datablocks::DataBlock;
use common_datavalues::prelude::*;
use common_exception::Result;
use common_meta_types::TableIdent;
use common_meta_types::TableInfo;
use common_meta_types::TableMeta;
use common_planners::ReadDataSourcePlan;
use common_streams::DataBlockStream;
use common_streams::SendableDataBlockStream;

use crate::catalogs::Table;
use crate::sessions::QueryContext;

pub struct UsersTable {
    table_info: TableInfo,
}

impl UsersTable {
    pub fn create(table_id: u64) -> Self {
        let schema = DataSchemaRefExt::create(vec![
            DataField::new("name", DataType::String, false),
            DataField::new("hostname", DataType::String, false),
            DataField::new("password", DataType::String, true),
            DataField::new("auth_type", DataType::UInt8, false),
        ]);

        let table_info = TableInfo {
            desc: "'system'.'users'".to_string(),
            name: "users".to_string(),
            ident: TableIdent::new(table_id, 0),
            meta: TableMeta {
                schema,
                engine: "SystemUsers".to_string(),
                ..Default::default()
            },
        };
        UsersTable { table_info }
    }
}

#[async_trait::async_trait]
impl Table for UsersTable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_table_info(&self) -> &TableInfo {
        &self.table_info
    }

    async fn read(
        &self,
        ctx: Arc<QueryContext>,
        _plan: &ReadDataSourcePlan,
    ) -> Result<SendableDataBlockStream> {
        let users = ctx
            .get_sessions_manager()
            .get_user_manager()
            .get_users()
            .await?;

        let names: Vec<&str> = users.iter().map(|x| x.name.as_str()).collect();
        let hostnames: Vec<&str> = users.iter().map(|x| x.hostname.as_str()).collect();
        let passwords: Vec<&[u8]> = users.iter().map(|x| x.password.as_slice()).collect();
        let auth_types: Vec<u8> = users.iter().map(|x| x.auth_type.clone() as u8).collect();
        let block = DataBlock::create_by_array(self.table_info.schema(), vec![
            Series::new(names),
            Series::new(hostnames),
            Series::new(passwords),
            Series::new(auth_types),
        ]);
        Ok(Box::pin(DataBlockStream::create(
            self.table_info.schema(),
            None,
            vec![block],
        )))
    }
}
