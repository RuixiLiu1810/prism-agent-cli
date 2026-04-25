use agent_core::{tools::error_result, AgentRuntimeState, AgentToolResult, MemoryEntry, MemoryType};
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::watch;
use uuid::Uuid;

use super::common::{ok_result, tool_arg_optional_string, tool_arg_string, truncate_preview};

pub(crate) async fn execute_remember_fact(
    runtime_state: &AgentRuntimeState,
    call_id: &str,
    args: Value,
    _cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    execute_memory_write_like("remember_fact", runtime_state, call_id, args).await
}

pub(crate) async fn execute_memory_write_alias(
    runtime_state: &AgentRuntimeState,
    call_id: &str,
    args: Value,
    _cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    execute_memory_write_like("memory_write", runtime_state, call_id, args).await
}

async fn execute_memory_write_like(
    tool_name: &str,
    runtime_state: &AgentRuntimeState,
    call_id: &str,
    args: Value,
) -> AgentToolResult {
    let content = tool_arg_string(&args, "content")
        .or_else(|_| tool_arg_string(&args, "value"))
        .map_err(|_| {
            "Missing required tool argument 'content' (or legacy alias 'value').".to_string()
        });
    let content = match content {
        Ok(value) => value,
        Err(message) => return error_result(tool_name, call_id, message),
    };

    let memory_type_raw = tool_arg_optional_string(&args, "memory_type")
        .unwrap_or_else(|| "reference".to_string());
    let memory_type = match memory_type_raw.as_str() {
        "user_preference" => MemoryType::UserPreference,
        "project_convention" => MemoryType::ProjectConvention,
        "correction" => MemoryType::Correction,
        _ => MemoryType::Reference,
    };

    let topic = tool_arg_optional_string(&args, "topic")
        .or_else(|| tool_arg_optional_string(&args, "key"));

    let now = Utc::now().to_rfc3339();
    let entry = MemoryEntry {
        id: Uuid::new_v4().to_string(),
        memory_type,
        content: content.clone(),
        topic,
        source_session: None,
        created_at: now.clone(),
        last_accessed: now,
    };

    match runtime_state.save_memory_entry(entry).await {
        Ok(()) => ok_result(
            tool_name,
            call_id,
            json!({
                "saved": true,
                "content": content,
            }),
            truncate_preview("Memory saved."),
        ),
        Err(err) => error_result(tool_name, call_id, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remember_fact_saves_entry() {
        let runtime = AgentRuntimeState::default();
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        runtime
            .ensure_storage_at(tmp.path().to_path_buf())
            .await
            .unwrap_or_else(|e| panic!("storage init: {e}"));

        let result = execute_remember_fact(
            &runtime,
            "call-1",
            json!({
                "content": "Use snake_case in Rust modules",
                "memory_type": "project_convention",
            }),
            None,
        )
        .await;
        assert!(!result.is_error, "result={:?}", result);
        assert_eq!(result.content["saved"], true);
    }

    #[tokio::test]
    async fn memory_write_alias_accepts_legacy_fields() {
        let runtime = AgentRuntimeState::default();
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        runtime
            .ensure_storage_at(tmp.path().to_path_buf())
            .await
            .unwrap_or_else(|e| panic!("storage init: {e}"));

        let result = execute_memory_write_alias(
            &runtime,
            "call-2",
            json!({
                "key": "coding-style",
                "value": "Prefer explicit error messages",
            }),
            None,
        )
        .await;
        assert!(!result.is_error, "result={:?}", result);
        assert_eq!(result.content["saved"], true);
    }
}
