mod common;
mod shell;
mod workspace;

use agent_core::{
    parse_tool_arguments, tools::error_result, AgentRuntimeState, AgentToolCall, AgentToolResult,
};
use tokio::sync::watch;

pub(crate) async fn execute_tool_call(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    project_root: &str,
    call: AgentToolCall,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    let parsed_args = match parse_tool_arguments(&call.arguments) {
        Ok(value) => value,
        Err(err) => {
            return error_result(
                &call.tool_name,
                &call.call_id,
                format!("Invalid tool arguments JSON: {}", err),
            );
        }
    };

    match call.tool_name.as_str() {
        "read_file" => {
            workspace::execute_read_file(project_root, &call.call_id, parsed_args, cancel_rx).await
        }
        "list_files" => {
            workspace::execute_list_files(project_root, &call.call_id, parsed_args, cancel_rx).await
        }
        "search_project" => {
            workspace::execute_search_project(project_root, &call.call_id, parsed_args, cancel_rx)
                .await
        }
        "run_shell_command" => {
            shell::execute_run_shell_command(
                runtime_state,
                tab_id,
                project_root,
                &call.call_id,
                parsed_args,
                cancel_rx,
            )
            .await
        }
        other => error_result(
            other,
            &call.call_id,
            format!(
                "Standalone agent-cli does not support tool '{}' in this runtime.",
                other
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::execute_tool_call;
    use agent_core::{AgentRuntimeState, AgentToolCall};

    #[tokio::test]
    async fn dispatches_read_file_tool() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        tokio::fs::write(dir.path().join("a.txt"), "hello")
            .await
            .unwrap_or_else(|e| panic!("write: {e}"));

        let result = execute_tool_call(
            &AgentRuntimeState::default(),
            "tab-1",
            dir.path().to_str().unwrap_or("."),
            AgentToolCall {
                tool_name: "read_file".to_string(),
                call_id: "call-1".to_string(),
                arguments: r#"{"path":"a.txt"}"#.to_string(),
            },
            None,
        )
        .await;

        assert!(!result.is_error);
        assert_eq!(result.content["content"], "hello");
    }

    #[tokio::test]
    async fn unknown_tool_remains_explicit_error() {
        let result = execute_tool_call(
            &AgentRuntimeState::default(),
            "tab-1",
            ".",
            AgentToolCall {
                tool_name: "write_file".to_string(),
                call_id: "call-1".to_string(),
                arguments: "{}".to_string(),
            },
            None,
        )
        .await;

        assert!(result.is_error);
        assert_eq!(result.tool_name, "write_file");
    }

    #[tokio::test]
    async fn dispatches_shell_tool() {
        let runtime = AgentRuntimeState::default();
        runtime
            .set_tool_approval("tab-1", "run_shell_command", "allow_once")
            .await
            .unwrap_or_else(|e| panic!("set approval: {e}"));

        let result = execute_tool_call(
            &runtime,
            "tab-1",
            ".",
            AgentToolCall {
                tool_name: "run_shell_command".to_string(),
                call_id: "call-1".to_string(),
                arguments: r#"{"command":"echo ok"}"#.to_string(),
            },
            None,
        )
        .await;

        assert!(!result.is_error, "result={:?}", result);
    }
}
