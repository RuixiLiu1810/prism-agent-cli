use std::sync::Arc;

use agent_core::{AgentRuntimeState, AgentToolCall, AgentToolResult};
use tokio::sync::watch;

use crate::local_tools;

pub async fn execute_cli_tool(
    runtime_state: Arc<AgentRuntimeState>,
    tab_id: String,
    project_root: String,
    call: AgentToolCall,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    local_tools::execute_tool_call(
        runtime_state.as_ref(),
        &tab_id,
        &project_root,
        call,
        cancel_rx,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_executor_returns_error_result() {
        let result = execute_cli_tool(
            Arc::new(AgentRuntimeState::default()),
            "tab-1".to_string(),
            ".".to_string(),
            AgentToolCall {
                tool_name: "write_file".to_string(),
                call_id: "call-1".to_string(),
                arguments: r#"{"path":"src/main.rs","content":"x"}"#.to_string(),
            },
            None,
        )
        .await;

        assert!(result.is_error);
        assert_eq!(result.tool_name, "write_file");
        assert_eq!(result.call_id, "call-1");
        assert!(result.preview.contains("does not support tool"));
    }
}
