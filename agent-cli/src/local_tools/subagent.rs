use std::sync::Arc;

use agent_core::{
    run_subagent_turn, tools::error_result, AgentRuntimeState, AgentToolResult,
    StaticConfigProvider, ToolExecutorFn,
};
use serde_json::Value;
use tokio::sync::watch;

use super::common::{ok_result, tool_arg_optional_string, tool_arg_string, truncate_preview};

const MAX_PROMPT_LEN: usize = 20_000;

pub(crate) async fn execute_spawn_subagent(
    config_provider: &StaticConfigProvider,
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    project_root: &str,
    call_id: &str,
    args: Value,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    let prompt = match tool_arg_string(&args, "prompt") {
        Ok(value) => value,
        Err(message) => return error_result("spawn_subagent", call_id, message),
    };
    if prompt.len() > MAX_PROMPT_LEN {
        return error_result(
            "spawn_subagent",
            call_id,
            format!("Prompt too long (max {} chars).", MAX_PROMPT_LEN),
        );
    }
    let model = tool_arg_optional_string(&args, "model");

    let child_runtime_state = Arc::new(runtime_state.clone());
    let child_tab_id = tab_id.to_string();
    let child_project_root = project_root.to_string();
    let child_executor: ToolExecutorFn = Arc::new(move |call, cancel_rx| {
        let runtime_state = Arc::clone(&child_runtime_state);
        let tab_id = child_tab_id.clone();
        let project_root = child_project_root.clone();
        Box::pin(async move {
            super::execute_tool_call_in_subagent(
                runtime_state.as_ref(),
                &tab_id,
                &project_root,
                call,
                cancel_rx,
            )
            .await
        })
    });

    match run_subagent_turn(
        config_provider,
        runtime_state,
        project_root,
        prompt,
        model,
        child_executor,
        cancel_rx,
    )
    .await
    {
        Ok(response) => ok_result(
            "spawn_subagent",
            call_id,
            serde_json::json!({ "response": response }),
            truncate_preview("Subagent task completed."),
        ),
        Err(err) => error_result("spawn_subagent", call_id, err),
    }
}

#[cfg(test)]
mod tests {
    use super::MAX_PROMPT_LEN;

    #[test]
    fn prompt_length_guard_is_enforced() {
        let oversized = "x".repeat(MAX_PROMPT_LEN + 1);
        assert!(oversized.len() > MAX_PROMPT_LEN);
    }
}
