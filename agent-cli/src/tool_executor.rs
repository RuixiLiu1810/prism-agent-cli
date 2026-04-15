use agent_core::{AgentToolCall, AgentToolResult};

pub fn execute_cli_tool(call: AgentToolCall) -> AgentToolResult {
    let tool_name = call.tool_name.clone();
    let call_id = call.call_id.clone();

    agent_core::tools::error_result(
        &tool_name,
        &call_id,
        format!(
            "Standalone agent-cli does not support tool '{tool_name}' in this runtime."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unsupported_executor_returns_error_result() {
        let result = execute_cli_tool(AgentToolCall {
            tool_name: "read_file".to_string(),
            call_id: "call-1".to_string(),
            arguments: r#"{"path":"src/main.rs"}"#.to_string(),
        });

        assert!(result.is_error);
        assert_eq!(result.tool_name, "read_file");
        assert_eq!(result.call_id, "call-1");
        assert_eq!(
            result.content,
            json!({
                "error": "Standalone agent-cli does not support tool 'read_file' in this runtime."
            })
        );
        assert_eq!(
            result.preview,
            "Standalone agent-cli does not support tool 'read_file' in this runtime."
        );
    }
}
