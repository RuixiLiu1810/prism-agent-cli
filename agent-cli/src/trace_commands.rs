use std::path::PathBuf;

use agent_core::AgentRuntimeState;
use serde_json::to_string_pretty;

use crate::command_router::TraceCommand;

pub async fn execute_trace_command(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    project_path: &str,
    command: TraceCommand,
) -> Result<String, String> {
    match command {
        TraceCommand::Show => show_trace(runtime_state, tab_id).await,
        TraceCommand::Clear => {
            runtime_state.clear_call_traces_for_tab(tab_id).await;
            Ok("Cleared in-memory call traces for current tab.".to_string())
        }
        TraceCommand::Export(path) => export_trace(runtime_state, tab_id, project_path, path).await,
    }
}

async fn show_trace(runtime_state: &AgentRuntimeState, tab_id: &str) -> Result<String, String> {
    let Some(mut trace) = runtime_state.latest_call_trace_for_tab(tab_id).await else {
        return Ok("No call trace available for current tab.".to_string());
    };
    if trace.ended_at.is_none() {
        trace.finalize("in_progress");
    }

    Ok(format!(
        "Trace summary\n- trace_id: {}\n- outcome: {}\n- spans: {}\n- duration_ms: {}\n- tool_calls: {}\n- suspends: {}\n- retries: {}\n- errors: {}",
        trace.trace_id,
        trace.outcome.as_deref().unwrap_or("unknown"),
        trace.spans.len(),
        trace.stats.duration_ms,
        trace.stats.tool_call_count,
        trace.stats.approval_suspend_count,
        trace.stats.retry_count,
        trace.stats.error_count,
    ))
}

async fn export_trace(
    runtime_state: &AgentRuntimeState,
    tab_id: &str,
    project_path: &str,
    path: Option<String>,
) -> Result<String, String> {
    let Some(mut trace) = runtime_state.latest_call_trace_for_tab(tab_id).await else {
        return Err("No call trace available to export.".to_string());
    };
    if trace.ended_at.is_none() {
        trace.finalize("in_progress");
    }

    let export_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_trace_export_path(project_path, &trace.trace_id));

    if let Some(parent) = export_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| format!("Failed to create export directory: {}", err))?;
    }

    let payload = to_string_pretty(&trace.to_export_value())
        .map_err(|err| format!("Failed to serialize trace JSON: {}", err))?;

    tokio::fs::write(&export_path, payload)
        .await
        .map_err(|err| format!("Failed to write trace export: {}", err))?;

    Ok(format!("Trace exported to {}", export_path.display()))
}

fn default_trace_export_path(project_path: &str, trace_id: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(".agent")
        .join("traces")
        .join(format!("{}.json", trace_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn show_returns_empty_state_message() {
        let runtime = AgentRuntimeState::default();
        let msg = execute_trace_command(&runtime, "tab-1", ".", TraceCommand::Show)
            .await
            .expect("show should succeed");
        assert!(msg.contains("No call trace"));
    }

    #[tokio::test]
    async fn export_writes_json_file() {
        let runtime = AgentRuntimeState::default();
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let _ = runtime
            .ensure_call_trace("tab-1", Some("session-1"), ".", "minimax", "M2.7")
            .await;
        let path = dir.path().join("trace.json");
        let msg = execute_trace_command(
            &runtime,
            "tab-1",
            ".",
            TraceCommand::Export(Some(path.to_string_lossy().to_string())),
        )
        .await
        .expect("export should succeed");
        assert!(msg.contains("Trace exported"));
        let written = tokio::fs::read_to_string(path)
            .await
            .unwrap_or_else(|e| panic!("read export: {e}"));
        assert!(written.contains("\"traceId\""));
    }
}
