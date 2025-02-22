//  Copyright 2021 Datafuse Labs.
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
//

use std::sync::Arc;

use common_exception::Result;

use crate::datasources::database::FuseDatabase;
use crate::datasources::database::GithubDatabase;
use crate::datasources::DatabaseEngineRegistry;

pub fn register_database_engines(registry: &DatabaseEngineRegistry) -> Result<()> {
    // Register a DEFAULT database engine.
    registry.register("DEFAULT", Arc::new(FuseDatabase::try_create))?;
    registry.register("GITHUB", Arc::new(GithubDatabase::try_create))?;
    Ok(())
}
