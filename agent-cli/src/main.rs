mod args;
mod output;
mod tool_executor;
mod turn_runner;

use std::{
    process::ExitCode,
    sync::Arc,
};

use agent_core::{
    emit_agent_complete, emit_error, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnProfile, EventSink,
    StaticConfigProvider, ToolExecutorFn,
};
use args::Args;
use clap::Parser;
use output::{HumanEventSink, JsonlEventSink};

fn emit_cli_failure(sink: &dyn EventSink, tab_id: &str, code: &str, message: &str) {
    emit_error(sink, tab_id, code, message.to_string());
    emit_agent_complete(sink, tab_id, "error");
}

fn default_base_url(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "minimax" => Some("https://api.minimax.chat/v1"),
        _ => None,
    }
}

fn completion_outcome(suspended: bool) -> &'static str {
    if suspended {
        "suspended"
    } else {
        "completed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{AgentCompletePayload, AgentEventEnvelope, AgentEventPayload};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<AgentEventEnvelope>>,
        completes: Mutex<Vec<AgentCompletePayload>>,
    }

    impl EventSink for RecordingSink {
        fn emit_event(&self, envelope: &AgentEventEnvelope) {
            self.events.lock().unwrap().push(envelope.clone());
        }

        fn emit_complete(&self, payload: &AgentCompletePayload) {
            self.completes.lock().unwrap().push(payload.clone());
        }
    }

    #[test]
    fn emit_cli_failure_emits_error_and_terminal_completion() {
        let sink = RecordingSink::default();

        emit_cli_failure(&sink, "tab-1", "turn_loop_failed", "network down");

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0].payload {
            AgentEventPayload::Error(error) => {
                assert_eq!(error.code, "turn_loop_failed");
                assert_eq!(error.message, "network down");
            }
            payload => panic!("expected error event, got {payload:?}"),
        }

        let completes = sink.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0].tab_id, "tab-1");
        assert_eq!(completes[0].outcome, "error");
    }

    #[test]
    fn completion_outcome_uses_completed_for_non_suspended_turns() {
        assert_eq!(completion_outcome(false), "completed");
        assert_eq!(completion_outcome(true), "suspended");
    }

    #[test]
    fn request_requires_tools_for_selection_edit_prompts() {
        let request = AgentTurnDescriptor {
            project_path: ".".to_string(),
            prompt: "[Selection: @src/main.rs:1:1-1:4]\nPlease edit this selection".to_string(),
            tab_id: "tab-1".to_string(),
            model: None,
            local_session_id: None,
            previous_response_id: None,
            turn_profile: None,
        };
        assert!(turn_runner::request_requires_tools(&request));
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    let provider = args.provider.trim().to_lowercase();

    let Some(default_base_url) = default_base_url(&provider) else {
        let message = format!(
            "Unsupported provider '{}'. Supported providers: minimax, deepseek.",
            args.provider
        );
        let fallback_sink = JsonlEventSink::stdout();
        emit_cli_failure(&fallback_sink, &args.tab_id, "unsupported_provider", &message);
        eprintln!("agent-runtime error: {message}");
        return ExitCode::FAILURE;
    };

    let output_mode = match args::parse_output_mode(&args.output) {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("agent-runtime error: {}", err);
            return ExitCode::FAILURE;
        }
    };

    let sink: Arc<dyn EventSink> = match output_mode {
        args::OutputMode::Human => Arc::new(HumanEventSink::stdout()),
        args::OutputMode::Jsonl => Arc::new(JsonlEventSink::stdout()),
    };

    let base_url = args
        .base_url
        .unwrap_or_else(|| default_base_url.to_string());

    let config = {
        let mut c = AgentRuntimeConfig::default_local_agent();
        c.provider = provider.clone();
        c.model = args.model.clone();
        c.api_key = Some(args.api_key);
        c.base_url = base_url;
        c
    };

    let config_provider = StaticConfigProvider {
        config,
        config_dir: std::env::temp_dir().join("agent-runtime"),
    };

    let runtime_state = AgentRuntimeState::default();
    let mut request = AgentTurnDescriptor {
        project_path: args.project_path,
        prompt: args.prompt.unwrap_or_default(),
        tab_id: args.tab_id.clone(),
        model: Some(args.model),
        local_session_id: Some(format!("{}-session", args.tab_id)),
        previous_response_id: None,
        turn_profile: None,
    };
    if turn_runner::request_requires_tools(&request) {
        let message = "This prompt requires tool execution, but agent-cli currently uses a fallback tool executor. Run from desktop runtime or use a suggestion-only prompt.".to_string();
        emit_cli_failure(sink.as_ref(), &request.tab_id, "tool_backend_unavailable", &message);
        eprintln!("agent-runtime error: {message}");
        return ExitCode::FAILURE;
    }
    // CLI currently uses a fallback tool executor; steer non-tool turns away from tool-calling.
    request.turn_profile = Some(AgentTurnProfile {
        task_kind: AgentTaskKind::SuggestionOnly,
        response_mode: AgentResponseMode::SuggestionOnly,
        ..AgentTurnProfile::default()
    });
    let tool_executor: ToolExecutorFn = Arc::new(|call, _cancel_rx| {
        Box::pin(async move { tool_executor::execute_cli_tool(call) })
    });

    let result = turn_runner::run_turn(
        sink.as_ref(),
        &config_provider,
        &runtime_state,
        &request,
        tool_executor,
    )
    .await;

    match result {
        Ok(outcome) => {
            emit_agent_complete(sink.as_ref(), &request.tab_id, completion_outcome(outcome.suspended));
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_cli_failure(sink.as_ref(), &request.tab_id, "turn_loop_failed", &error);
            eprintln!("agent-runtime error: {error}");
            ExitCode::FAILURE
        }
    }
}
