use std::sync::Arc;

use agent_core::{
    emit_turn_resumed, providers, AgentRuntimeState, AgentTurnDescriptor, CallSpanStatus,
    CallSpanType, EventSink, PendingTurnResume, StaticConfigProvider, ToolExecutorFn,
};
use serde_json::json;

pub fn is_chat_completions_provider(provider: &str) -> bool {
    matches!(provider, "minimax" | "deepseek")
}

pub fn request_requires_tools(request: &AgentTurnDescriptor) -> bool {
    let profile = agent_core::resolve_turn_profile(request);
    agent_core::tool_choice_for_task(request, &profile) == "required"
}

pub async fn run_turn(
    sink: &dyn EventSink,
    config_provider: &StaticConfigProvider,
    runtime_state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    tool_executor: ToolExecutorFn,
) -> Result<providers::AgentTurnOutcome, String> {
    let provider = config_provider.config.provider.trim().to_ascii_lowercase();
    let model = request
        .model
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config_provider.config.model.clone());
    let _trace_id = runtime_state
        .ensure_call_trace(
            &request.tab_id,
            request.local_session_id.as_deref(),
            &request.project_path,
            &provider,
            &model,
        )
        .await;
    let turn_span_id = runtime_state
        .trace_start_span(
            &request.tab_id,
            CallSpanType::Turn,
            "turn",
            None,
            json!({
                "provider": provider,
                "model": model,
                "tab_id": request.tab_id,
                "session_id": request.local_session_id,
            }),
        )
        .await;

    if !is_chat_completions_provider(&provider) {
        if let Some(span_id) = turn_span_id {
            let _ = runtime_state
                .trace_close_span(
                    &request.tab_id,
                    &span_id,
                    CallSpanStatus::Error,
                    Some(json!({"error": "unsupported_provider"})),
                )
                .await;
        }
        runtime_state
            .trace_record_error(
                &request.tab_id,
                "unsupported_provider",
                "Provider is not enabled for MVP-1 REPL",
            )
            .await;
        let _ = runtime_state.finalize_call_trace(&request.tab_id, "error").await;
        return Err(format!(
            "Provider '{}' is not enabled for MVP-1 REPL. Use minimax or deepseek.",
            provider
        ));
    }

    let history = if let Some(local_session_id) = request.local_session_id.as_deref() {
        runtime_state
            .history_for_session(local_session_id)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let result = providers::chat_completions::run_turn_loop(
        sink,
        config_provider,
        runtime_state,
        request,
        &history,
        Arc::clone(&tool_executor),
        None,
    )
    .await;

    match &result {
        Ok(outcome) if outcome.suspended => {
            if let Some(span_id) = turn_span_id.clone() {
                let _ = runtime_state
                    .trace_close_span(
                        &request.tab_id,
                        &span_id,
                        CallSpanStatus::Interrupted,
                        Some(json!({ "outcome": "suspended" })),
                    )
                    .await;
            }
        }
        Ok(_) => {
            if let Some(span_id) = turn_span_id.clone() {
                let _ = runtime_state
                    .trace_close_span(
                        &request.tab_id,
                        &span_id,
                        CallSpanStatus::Ok,
                        Some(json!({ "outcome": "completed" })),
                    )
                    .await;
            }
            let _ = runtime_state
                .finalize_call_trace(&request.tab_id, "completed")
                .await;
        }
        Err(err) => {
            let span_status = if err == agent_core::AGENT_CANCELLED_MESSAGE {
                CallSpanStatus::Cancelled
            } else {
                CallSpanStatus::Error
            };
            if let Some(span_id) = turn_span_id {
                let _ = runtime_state
                    .trace_close_span(
                        &request.tab_id,
                        &span_id,
                        span_status,
                        Some(json!({ "error": err })),
                    )
                    .await;
            }
            runtime_state
                .trace_record_error(&request.tab_id, "turn_error", err)
                .await;
            let final_outcome = if err == agent_core::AGENT_CANCELLED_MESSAGE {
                "cancelled"
            } else {
                "error"
            };
            let _ = runtime_state
                .finalize_call_trace(&request.tab_id, final_outcome)
                .await;
        }
    }
    let outcome = result?;

    if let Some(local_session_id) = request.local_session_id.as_deref() {
        runtime_state
            .append_history(local_session_id, outcome.messages.clone())
            .await;
    }

    Ok(outcome)
}

pub async fn resume_pending_turn(
    sink: &dyn EventSink,
    config_provider: &StaticConfigProvider,
    runtime_state: &AgentRuntimeState,
    pending: PendingTurnResume,
    fallback_model: &str,
    fallback_local_session_id: &str,
    tool_executor: ToolExecutorFn,
) -> Result<providers::AgentTurnOutcome, String> {
    let local_session_id = pending
        .local_session_id
        .clone()
        .unwrap_or_else(|| fallback_local_session_id.to_string());
    let model = pending
        .model
        .clone()
        .unwrap_or_else(|| fallback_model.to_string());

    emit_turn_resumed(
        sink,
        &pending.tab_id,
        Some(local_session_id.as_str()),
        &format!(
            "Approval received for {}. Resuming suspended turn.",
            pending.approval_tool_name
        ),
    );
    if let Some(span_id) = runtime_state
        .trace_start_span(
            &pending.tab_id,
            CallSpanType::TurnResume,
            "turn_resume",
            None,
            json!({
                "approval_tool_name": pending.approval_tool_name,
                "local_session_id": local_session_id,
            }),
        )
        .await
    {
        let _ = runtime_state
            .trace_close_span(
                &pending.tab_id,
                &span_id,
                CallSpanStatus::Ok,
                Some(json!({ "message": "resumed" })),
            )
            .await;
    }

    let request = AgentTurnDescriptor {
        project_path: pending.project_path,
        prompt: pending.continuation_prompt,
        tab_id: pending.tab_id,
        model: Some(model),
        local_session_id: Some(local_session_id),
        previous_response_id: None,
        turn_profile: pending.turn_profile,
    };

    run_turn(
        sink,
        config_provider,
        runtime_state,
        &request,
        Arc::clone(&tool_executor),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{is_chat_completions_provider, request_requires_tools};
    use agent_core::AgentTurnDescriptor;

    #[test]
    fn accepts_minimax_and_deepseek_only() {
        assert!(is_chat_completions_provider("minimax"));
        assert!(is_chat_completions_provider("deepseek"));
        assert!(!is_chat_completions_provider("openai"));
    }

    #[test]
    fn detects_tool_required_requests() {
        let req = AgentTurnDescriptor {
            project_path: ".".to_string(),
            prompt: "[Selection: @src/main.rs:1:1-1:2]\nedit this".to_string(),
            tab_id: "t1".to_string(),
            model: None,
            local_session_id: None,
            previous_response_id: None,
            turn_profile: None,
        };
        assert!(request_requires_tools(&req));
    }
}
