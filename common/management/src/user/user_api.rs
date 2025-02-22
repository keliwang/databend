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
//

use common_exception::Result;
use common_meta_types::AuthType;
use common_meta_types::SeqV;
use common_meta_types::UserInfo;
use common_meta_types::UserPrivilege;

#[async_trait::async_trait]
pub trait UserMgrApi: Sync + Send {
    async fn add_user(&self, user_info: UserInfo) -> Result<u64>;

    async fn get_user(
        &self,
        username: String,
        hostname: String,
        seq: Option<u64>,
    ) -> Result<SeqV<UserInfo>>;

    async fn get_users(&self) -> Result<Vec<SeqV<UserInfo>>>;

    async fn update_user(
        &self,
        username: String,
        hostname: String,
        new_password: Option<Vec<u8>>,
        new_auth: Option<AuthType>,
        seq: Option<u64>,
    ) -> Result<Option<u64>>;

    async fn set_user_privileges(
        &self,
        username: String,
        hostname: String,
        privileges: UserPrivilege,
        seq: Option<u64>,
    ) -> Result<Option<u64>>;

    async fn drop_user(&self, username: String, hostname: String, seq: Option<u64>) -> Result<()>;
}
