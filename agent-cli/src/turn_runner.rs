use std::sync::Arc;

use agent_core::{
    emit_turn_resumed, providers, AgentRuntimeState, AgentTurnDescriptor, EventSink,
    PendingTurnResume, StaticConfigProvider, ToolExecutorFn,
};

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
    if !is_chat_completions_provider(&provider) {
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

    let outcome = providers::chat_completions::run_turn_loop(
        sink,
        config_provider,
        runtime_state,
        request,
        &history,
        Arc::clone(&tool_executor),
        None,
    )
    .await?;

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
