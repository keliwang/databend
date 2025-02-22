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

use std::sync::Arc;

use common_datavalues::DataSchemaRef;
use common_exception::Result;
use common_planners::PlanNode;
use common_planners::SelectPlan;
use common_streams::SendableDataBlockStream;
use common_tracing::tracing;

use super::utils::apply_plan_rewrite;
use crate::interpreters::plan_scheduler_ext;
use crate::interpreters::Interpreter;
use crate::interpreters::InterpreterPtr;
use crate::optimizers::Optimizers;
use crate::sessions::QueryContext;

pub struct SelectInterpreter {
    ctx: Arc<QueryContext>,
    select: SelectPlan,
}

impl SelectInterpreter {
    pub fn try_create(ctx: Arc<QueryContext>, select: SelectPlan) -> Result<InterpreterPtr> {
        Ok(Arc::new(SelectInterpreter { ctx, select }))
    }

    fn rewrite_plan(&self) -> Result<PlanNode> {
        apply_plan_rewrite(Optimizers::create(self.ctx.clone()), &self.select.input)
    }
}

#[async_trait::async_trait]
impl Interpreter for SelectInterpreter {
    fn name(&self) -> &str {
        "SelectInterpreter"
    }

    #[tracing::instrument(level = "info", skip(self, _input_stream), fields(ctx.id = self.ctx.get_id().as_str()))]
    async fn execute(
        &self,
        _input_stream: Option<SendableDataBlockStream>,
    ) -> Result<SendableDataBlockStream> {
        // TODO: maybe panic?
        let optimized_plan = self.rewrite_plan()?;
        plan_scheduler_ext::schedule_query(&self.ctx, &optimized_plan).await
    }

    fn schema(&self) -> DataSchemaRef {
        self.select.schema()
    }
}
