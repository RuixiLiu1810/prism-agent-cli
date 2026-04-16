use std::sync::Arc;

use agent_core::{
    providers, AgentRuntimeState, AgentTurnDescriptor, EventSink, StaticConfigProvider,
    ToolExecutorFn,
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
