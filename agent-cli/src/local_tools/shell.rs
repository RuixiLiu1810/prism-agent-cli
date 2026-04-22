use std::time::Duration;

use agent_core::{tools::error_result, AgentRuntimeState, AgentToolResult};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::watch;

use super::common::{command_output_with_cancel, ok_result, tool_arg_string, truncate_preview};

const SHELL_COMMAND_TIMEOUT_SECS: u64 = 30;
const SHELL_OUTPUT_MAX_BYTES: usize = 32_000;

const BLOCKED_SHELL_PATTERNS: &[&str] = &[
    "rm -rf",
    "sudo ",
    "chmod 777",
    "dd ",
    "mkfs",
    "curl | bash",
    "curl|bash",
    "wget | sh",
    "wget|sh",
    "> /dev/",
    ":(){ :",
];

const ALLOWED_SHELL_COMMANDS: &[&str] = &[
    "grep", "rg", "wc", "cat", "head", "tail", "ls", "find", "diff", "echo", "mkdir", "cp", "mv",
    "touch", "sort", "uniq", "sed", "awk", "python", "python3", "pip", "pip3", "uv", "node", "npm",
    "npx", "git",
];

fn approval_required_result(
    tool_name: &str,
    call_id: &str,
    reason: String,
    args: Value,
) -> AgentToolResult {
    AgentToolResult {
        tool_name: tool_name.to_string(),
        call_id: call_id.to_string(),
        is_error: true,
        preview: truncate_preview(&reason),
        content: json!({
            "approvalRequired": true,
            "toolName": tool_name,
            "reason": reason,
            "input": args,
        }),
    }
}

fn is_blocked_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    BLOCKED_SHELL_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

fn extract_first_command_token(command: &str) -> &str {
    let trimmed = command.trim();
    let cmd_start = trimmed
        .split_whitespace()
        .find(|token| !token.contains('='))
        .unwrap_or(trimmed);
    cmd_start.rsplit('/').next().unwrap_or(cmd_start)
}

fn is_allowed_command(command: &str) -> bool {
    let first_token = extract_first_command_token(command);
    ALLOWED_SHELL_COMMANDS.contains(&first_token)
}

fn truncate_command_output(bytes: &[u8], max_bytes: usize) -> (String, bool) {
    if bytes.len() <= max_bytes {
        return (String::from_utf8_lossy(bytes).to_string(), false);
    }

    let mut end = max_bytes.min(bytes.len());
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }

    let mut text = String::from_utf8_lossy(&bytes[..end]).to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("...[truncated]");
    (text, true)
}

pub(crate) async fn execute_run_shell_command(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    project_root: &str,
    call_id: &str,
    args: Value,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    let command = match tool_arg_string(&args, "command") {
        Ok(value) => value,
        Err(message) => return error_result("run_shell_command", call_id, message),
    };

    if is_blocked_command(&command) {
        return error_result(
            "run_shell_command",
            call_id,
            format!(
                "Command blocked for safety: '{}'. This command matches a dangerous pattern and cannot be executed.",
                truncate_preview(&command)
            ),
        );
    }

    let approval = runtime_state
        .check_tool_approval(tab_id, "run_shell_command")
        .await;

    if approval.deny_session {
        return approval_required_result(
            "run_shell_command",
            call_id,
            "run_shell_command is denied for this chat session.".to_string(),
            args,
        );
    }

    if !approval.allow_session && approval.allow_once_remaining == 0 {
        return approval_required_result(
            "run_shell_command",
            call_id,
            "run_shell_command requires approval before the command can continue.".to_string(),
            args,
        );
    }

    if !is_allowed_command(&command) {
        return approval_required_result(
            "run_shell_command",
            call_id,
            format!(
                "Command '{}' is not in the safe allowlist. Requires explicit approval.",
                extract_first_command_token(&command)
            ),
            json!({ "command": command }),
        );
    }

    #[cfg(not(target_os = "windows"))]
    let (shell, shell_args) = ("sh", vec!["-c".to_string(), command.clone()]);
    #[cfg(target_os = "windows")]
    let (shell, shell_args) = ("cmd", vec!["/C".to_string(), command.clone()]);

    let mut cmd = Command::new(shell);
    cmd.args(shell_args).current_dir(project_root);

    let output_result = tokio::time::timeout(
        Duration::from_secs(SHELL_COMMAND_TIMEOUT_SECS),
        command_output_with_cancel(
            cmd,
            cancel_rx,
            "run_shell_command",
            call_id,
            "Failed to spawn shell command",
        ),
    )
    .await;

    let output = match output_result {
        Ok(Ok(output)) => output,
        Ok(Err(result)) => return result,
        Err(_) => {
            return error_result(
                "run_shell_command",
                call_id,
                format!(
                    "Shell command timed out after {} seconds.",
                    SHELL_COMMAND_TIMEOUT_SECS
                ),
            );
        }
    };

    let (stdout, stdout_truncated) =
        truncate_command_output(&output.stdout, SHELL_OUTPUT_MAX_BYTES);
    let (stderr, stderr_truncated) =
        truncate_command_output(&output.stderr, SHELL_OUTPUT_MAX_BYTES);
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout_preview = truncate_preview(&stdout);
    let stderr_preview = truncate_preview(&stderr);

    ok_result(
        "run_shell_command",
        call_id,
        json!({
            "command": command,
            "exitCode": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "stdoutTruncated": stdout_truncated,
            "stderrTruncated": stderr_truncated,
            "timeoutSecs": SHELL_COMMAND_TIMEOUT_SECS,
            "outputMaxBytes": SHELL_OUTPUT_MAX_BYTES,
        }),
        format!(
            "exit={} stdout={} stderr={}",
            exit_code, stdout_preview, stderr_preview
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::execute_run_shell_command;
    use agent_core::AgentRuntimeState;

    #[tokio::test]
    async fn blocks_dangerous_shell_pattern() {
        let runtime = AgentRuntimeState::default();
        let result = execute_run_shell_command(
            &runtime,
            "tab-1",
            ".",
            "call-1",
            serde_json::json!({"command":"rm -rf /tmp/test"}),
            None,
        )
        .await;

        assert!(result.is_error);
        assert!(result.preview.contains("blocked for safety"));
    }

    #[tokio::test]
    async fn requires_approval_for_shell() {
        let runtime = AgentRuntimeState::default();
        let result = execute_run_shell_command(
            &runtime,
            "tab-1",
            ".",
            "call-1",
            serde_json::json!({"command":"echo ok"}),
            None,
        )
        .await;

        assert!(result.is_error);
        assert_eq!(result.content["approvalRequired"], true);
    }

    #[tokio::test]
    async fn executes_shell_after_allow_once_approval() {
        let runtime = AgentRuntimeState::default();
        runtime
            .set_tool_approval("tab-1", "run_shell_command", "allow_once")
            .await
            .unwrap_or_else(|e| panic!("set approval: {e}"));

        let result = execute_run_shell_command(
            &runtime,
            "tab-1",
            ".",
            "call-1",
            serde_json::json!({"command":"echo ok"}),
            None,
        )
        .await;

        assert!(!result.is_error, "result={:?}", result);
        assert_eq!(result.content["exitCode"], 0);
    }

    #[tokio::test]
    async fn cancel_signal_returns_cancelled_error() {
        let runtime = AgentRuntimeState::default();
        runtime
            .set_tool_approval("tab-1", "run_shell_command", "allow_once")
            .await
            .unwrap_or_else(|e| panic!("set approval: {e}"));

        let (tx, rx) = tokio::sync::watch::channel(false);
        tx.send(true).unwrap_or_else(|e| panic!("cancel send: {e}"));

        let result = execute_run_shell_command(
            &runtime,
            "tab-1",
            ".",
            "call-1",
            serde_json::json!({"command":"echo ok"}),
            Some(rx),
        )
        .await;

        assert!(result.is_error);
        assert!(result.preview.contains("Agent run cancelled by user."));
    }
}
