use serde_json::Value;
use tokio::sync::watch;
use uuid::Uuid;

use crate::event_sink::NullEventSink;
use crate::provider::AgentTurnDescriptor;
use crate::providers;
use crate::{AgentRuntimeState, StaticConfigProvider, ToolExecutorFn};

fn extract_last_assistant_text(messages: &[Value]) -> Option<String> {
    messages.iter().rev().find_map(|item| {
        if item.get("type").and_then(Value::as_str) != Some("assistant") {
            return None;
        }
        let content = crate::extract_text_segments(item).join("\n\n");
        let trimmed = content.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub async fn run_subagent_turn(
    config_provider: &StaticConfigProvider,
    runtime_state: &AgentRuntimeState,
    project_path: &str,
    prompt: String,
    model: Option<String>,
    tool_executor: ToolExecutorFn,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<String, String> {
    let tab_id = format!("subagent-{}", Uuid::new_v4());
    let request = AgentTurnDescriptor {
        project_path: project_path.to_string(),
        prompt,
        tab_id: tab_id.clone(),
        model,
        local_session_id: Some(format!("{tab_id}-session")),
        previous_response_id: None,
        turn_profile: None,
    };
    let sink = NullEventSink;

    let provider = config_provider.config.provider.trim().to_ascii_lowercase();
    let outcome = if matches!(provider.as_str(), "minimax" | "deepseek") {
        providers::chat_completions::run_turn_loop(
            &sink,
            config_provider,
            runtime_state,
            &request,
            &[],
            tool_executor,
            cancel_rx,
        )
        .await?
    } else if provider == "openai" {
        providers::openai::run_turn_loop(
            &sink,
            config_provider,
            runtime_state,
            &request,
            tool_executor,
            cancel_rx,
        )
        .await?
    } else {
        return Err(format!(
            "Provider '{}' is not supported for subagent execution.",
            provider
        ));
    };

    extract_last_assistant_text(&outcome.messages)
        .ok_or_else(|| "Subagent finished without assistant text output.".to_string())
}

#[cfg(test)]
mod tests {
    use super::extract_last_assistant_text;
    use serde_json::json;

    #[test]
    fn extracts_latest_non_empty_assistant_text() {
        let messages = vec![
            json!({
                "type": "assistant",
                "message": { "content": [{ "type": "text", "text": "" }] }
            }),
            json!({
                "type": "assistant",
                "message": { "content": [{ "type": "text", "text": "subagent answer" }] }
            }),
        ];
        assert_eq!(
            extract_last_assistant_text(&messages),
            Some("subagent answer".to_string())
        );
    }

    #[test]
    fn returns_none_when_no_assistant_text_present() {
        let messages = vec![json!({
            "type": "user",
            "message": { "content": [{ "type": "text", "text": "hello" }] }
        })];
        assert_eq!(extract_last_assistant_text(&messages), None);
    }
}
