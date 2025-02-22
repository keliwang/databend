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

use common_exception::Result;

use crate::functions::ContextFunction;

#[test]
fn test_context_function_build_arg_from_ctx() -> Result<()> {
    use pretty_assertions::assert_eq;
    let ctx = crate::tests::try_create_context()?;

    // Ok.
    {
        let args = ContextFunction::build_args_from_ctx("database", ctx.clone())?;
        assert_eq!("default", format!("{:?}", args[0]));
    }

    // Ok.
    {
        let args = ContextFunction::build_args_from_ctx("current_user", ctx.clone())?;
        assert_eq!("", format!("{:?}", args[0]));
    }

    // Error.
    {
        let result = ContextFunction::build_args_from_ctx("databasexx", ctx).is_err();
        assert!(result);
    }

    Ok(())
}
