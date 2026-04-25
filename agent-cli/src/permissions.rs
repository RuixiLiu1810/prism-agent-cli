use std::collections::BTreeMap;

use crate::command_router::PermissionCommand;
use agent_core::{AgentRuntimeState, PendingTurnResume, ToolApprovalDecision, ToolApprovalRecord};

#[derive(Debug, Clone)]
pub struct PermissionAction {
    pub message: String,
    pub pending_turn: Option<PendingTurnResume>,
}

pub async fn execute_permission_command(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    command: PermissionCommand,
) -> Result<PermissionAction, String> {
    match command {
        PermissionCommand::Show => {
            let approvals = runtime_state.tool_approvals_for_tab(tab_id).await;
            let pending = runtime_state.pending_turn_for(tab_id).await;
            Ok(PermissionAction {
                message: format_permissions_status(tab_id, approvals, pending),
                pending_turn: None,
            })
        }
        PermissionCommand::ShellOnce => {
            apply_with_optional_resume(
                runtime_state,
                tab_id,
                "allow_once",
                "Approved shell for one command in this session.",
            )
            .await
        }
        PermissionCommand::ShellSession => {
            apply_with_optional_resume(
                runtime_state,
                tab_id,
                "allow_session",
                "Approved shell for this session.",
            )
            .await
        }
        PermissionCommand::ShellDeny => {
            apply_with_optional_resume(
                runtime_state,
                tab_id,
                "deny_session",
                "Denied shell for this session.",
            )
            .await
        }
        PermissionCommand::Clear => {
            runtime_state.clear_tool_approvals(tab_id).await;
            Ok(PermissionAction {
                message: "Cleared session permission overrides.".to_string(),
                pending_turn: None,
            })
        }
    }
}

fn format_permissions_status(
    tab_id: &str,
    approvals: std::collections::HashMap<String, ToolApprovalRecord>,
    pending: Option<PendingTurnResume>,
) -> String {
    let mut lines = vec![format!("Permissions (tab: {tab_id})")];
    if approvals.is_empty() {
        lines.push("- rules: none".to_string());
    } else {
        lines.push("- rules:".to_string());
        let sorted = approvals
            .into_iter()
            .collect::<BTreeMap<String, ToolApprovalRecord>>();
        for (tool_name, record) in sorted {
            lines.push(format!("  - {tool_name}: {}", format_record(&record)));
        }
    }

    if let Some(pending_turn) = pending {
        lines.push("- pending turn: yes".to_string());
        lines.push(format!("  - tool: {}", pending_turn.approval_tool_name));
        if let Some(target) = pending_turn.target_label {
            lines.push(format!("  - target: {target}"));
        }
        if let Some(session_id) = pending_turn.local_session_id {
            lines.push(format!("  - session: {session_id}"));
        }
        lines.push(
            "  - next: /permissions shell once|session|deny (or /approve shell ...)".to_string(),
        );
    } else {
        lines.push("- pending turn: none".to_string());
    }

    lines.join("\n")
}

fn format_record(record: &ToolApprovalRecord) -> String {
    match record.decision {
        ToolApprovalDecision::AllowSession => "allow_session".to_string(),
        ToolApprovalDecision::DenySession => "deny_session".to_string(),
        ToolApprovalDecision::AllowOnce => format!(
            "allow_once (remaining={}, expires={})",
            record.remaining_uses,
            record.expires_at.as_deref().unwrap_or("n/a")
        ),
    }
}

async fn apply_with_optional_resume(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    decision: &str,
    success_message: &str,
) -> Result<PermissionAction, String> {
    runtime_state
        .set_tool_approval(tab_id, "run_shell_command", decision)
        .await?;
    let pending_turn = runtime_state.take_pending_turn(tab_id).await;
    let message = if pending_turn.is_some() {
        format!("{success_message} Resuming suspended turn...")
    } else {
        format!("{success_message} No suspended turn found.")
    };
    Ok(PermissionAction {
        message,
        pending_turn,
    })
}

#[cfg(test)]
mod tests {
    use super::execute_permission_command;
    use crate::command_router::PermissionCommand;
    use agent_core::{AgentRuntimeState, PendingTurnResume};

    #[tokio::test]
    async fn show_reports_pending_turn_without_consuming_it() {
        let runtime_state = AgentRuntimeState::default();
        runtime_state
            .store_pending_turn(PendingTurnResume {
                project_path: ".".to_string(),
                tab_id: "tab-1".to_string(),
                local_session_id: Some("session-1".to_string()),
                model: Some("MiniMax-M2.7".to_string()),
                turn_profile: None,
                approval_tool_name: "run_shell_command".to_string(),
                target_label: Some("README.md".to_string()),
                continuation_prompt: "Continue.".to_string(),
                created_at: String::new(),
                expires_at: String::new(),
            })
            .await;

        let result = execute_permission_command(&runtime_state, "tab-1", PermissionCommand::Show)
            .await
            .expect("show should work");
        assert!(result.message.contains("pending turn: yes"));
        assert!(runtime_state.pending_turn_for("tab-1").await.is_some());
    }

    #[tokio::test]
    async fn apply_shell_once_returns_pending_turn_for_resume() {
        let runtime_state = AgentRuntimeState::default();
        runtime_state
            .store_pending_turn(PendingTurnResume {
                project_path: ".".to_string(),
                tab_id: "tab-1".to_string(),
                local_session_id: Some("session-1".to_string()),
                model: Some("MiniMax-M2.7".to_string()),
                turn_profile: None,
                approval_tool_name: "run_shell_command".to_string(),
                target_label: None,
                continuation_prompt: "Continue.".to_string(),
                created_at: String::new(),
                expires_at: String::new(),
            })
            .await;

        let result =
            execute_permission_command(&runtime_state, "tab-1", PermissionCommand::ShellOnce)
                .await
                .expect("shell once should work");
        assert!(result.pending_turn.is_some());
        assert!(result.message.contains("Resuming suspended turn"));
    }
}
