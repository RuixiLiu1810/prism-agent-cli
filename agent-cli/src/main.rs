mod args;
mod config_model;
mod config_resolver;
mod config_store;
mod config_wizard;
mod output;
mod repl;
mod tool_executor;
mod turn_runner;

use std::{process::ExitCode, sync::Arc};

use agent_core::{
    emit_agent_complete, emit_error, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnProfile, EventSink, StaticConfigProvider,
    ToolExecutorFn,
};
use args::{Args, RunMode};
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

fn build_request(args: &Args, prompt: String, local_session_id: &str) -> AgentTurnDescriptor {
    let mut request = AgentTurnDescriptor {
        project_path: args.project_path.clone(),
        prompt,
        tab_id: args.tab_id.clone(),
        model: args.model.clone(),
        local_session_id: Some(local_session_id.to_string()),
        previous_response_id: None,
        turn_profile: None,
    };

    // MVP-1 keeps fallback tool executor; force suggestion-only to avoid required tool calls.
    request.turn_profile = Some(AgentTurnProfile {
        task_kind: AgentTaskKind::SuggestionOnly,
        response_mode: AgentResponseMode::SuggestionOnly,
        ..AgentTurnProfile::default()
    });

    request
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
            let mut guard = match self.events.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.push(envelope.clone());
        }

        fn emit_complete(&self, payload: &AgentCompletePayload) {
            let mut guard = match self.completes.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.push(payload.clone());
        }
    }

    #[test]
    fn emit_cli_failure_emits_error_and_terminal_completion() {
        let sink = RecordingSink::default();

        emit_cli_failure(&sink, "tab-1", "turn_loop_failed", "network down");

        let events = match sink.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert_eq!(events.len(), 1);
        match &events[0].payload {
            AgentEventPayload::Error(error) => {
                assert_eq!(error.code, "turn_loop_failed");
                assert_eq!(error.message, "network down");
            }
            payload => panic!("expected error event, got {payload:?}"),
        }

        let completes = match sink.completes.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
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

    #[test]
    fn default_base_url_prefers_chat_completions_endpoints() {
        assert_eq!(
            default_base_url("minimax"),
            Some("https://api.minimax.chat/v1")
        );
        assert_eq!(
            default_base_url("deepseek"),
            Some("https://api.deepseek.com/v1")
        );
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    if let RunMode::Command = args.run_mode() {
        eprintln!("agent-runtime error: config command path not wired yet");
        return ExitCode::FAILURE;
    }

    let provider = args
        .provider
        .clone()
        .unwrap_or_else(|| "minimax".to_string())
        .trim()
        .to_ascii_lowercase();

    let Some(default_url) = default_base_url(&provider) else {
        let message = format!(
            "Unsupported provider '{}'. Supported providers: minimax, deepseek.",
            args.provider.as_deref().unwrap_or("<unset>")
        );
        let fallback_sink = JsonlEventSink::stdout();
        emit_cli_failure(&fallback_sink, &args.tab_id, "unsupported_provider", &message);
        eprintln!("agent-runtime error: {message}");
        return ExitCode::FAILURE;
    };

    let output_mode = match args::parse_output_mode(args.output.as_deref().unwrap_or("human")) {
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

    let mut config = AgentRuntimeConfig::default_local_agent();
    config.provider = provider;
    config.model = args
        .model
        .clone()
        .unwrap_or_else(|| "MiniMax-M1".to_string());
    config.api_key = args.api_key.clone();
    config.base_url = args
        .base_url
        .clone()
        .unwrap_or_else(|| default_url.to_string());

    let config_provider = Arc::new(StaticConfigProvider {
        config,
        config_dir: std::env::temp_dir().join("agent-runtime"),
    });
    let runtime_state = Arc::new(AgentRuntimeState::default());

    let tool_executor: ToolExecutorFn = Arc::new(|call, _cancel_rx| {
        Box::pin(async move { tool_executor::execute_cli_tool(call) })
    });

    let local_session_id = format!("{}-session", args.tab_id);

    match args.run_mode() {
        RunMode::SingleTurn => {
            let prompt = args.prompt.clone().unwrap_or_default();
            let request = build_request(&args, prompt, &local_session_id);
            if turn_runner::request_requires_tools(&request) {
                let message = "This prompt requires tool execution, but agent-cli currently uses a fallback tool executor. Run from desktop runtime or use a suggestion-only prompt.".to_string();
                emit_cli_failure(
                    sink.as_ref(),
                    &request.tab_id,
                    "tool_backend_unavailable",
                    &message,
                );
                eprintln!("agent-runtime error: {message}");
                return ExitCode::FAILURE;
            }

            match turn_runner::run_turn(
                sink.as_ref(),
                config_provider.as_ref(),
                runtime_state.as_ref(),
                &request,
                Arc::clone(&tool_executor),
            )
            .await
            {
                Ok(outcome) => {
                    emit_agent_complete(
                        sink.as_ref(),
                        &request.tab_id,
                        completion_outcome(outcome.suspended),
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    emit_cli_failure(sink.as_ref(), &args.tab_id, "turn_loop_failed", &error);
                    eprintln!("agent-runtime error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        RunMode::Repl => {
            let mut stdout = std::io::stdout();
            let reader = repl::stdin_reader();
            let repl_args = args.clone();
            let repl_session_id = local_session_id.clone();
            let repl_sink = Arc::clone(&sink);
            let repl_config_provider = Arc::clone(&config_provider);
            let repl_runtime_state = Arc::clone(&runtime_state);
            let repl_tool_executor = Arc::clone(&tool_executor);

            let res = repl::run_repl(reader, &mut stdout, move |prompt| {
                let request = build_request(&repl_args, prompt, &repl_session_id);
                let sink = Arc::clone(&repl_sink);
                let config_provider = Arc::clone(&repl_config_provider);
                let runtime_state = Arc::clone(&repl_runtime_state);
                let tool_executor = Arc::clone(&repl_tool_executor);

                Box::pin(async move {
                    if turn_runner::request_requires_tools(&request) {
                        let message = "This prompt requires tool execution, but agent-cli currently uses a fallback tool executor. Run from desktop runtime or use a suggestion-only prompt.".to_string();
                        emit_cli_failure(
                            sink.as_ref(),
                            &request.tab_id,
                            "tool_backend_unavailable",
                            &message,
                        );
                        return Ok(());
                    }

                    match turn_runner::run_turn(
                        sink.as_ref(),
                        config_provider.as_ref(),
                        runtime_state.as_ref(),
                        &request,
                        tool_executor,
                    )
                    .await
                    {
                        Ok(outcome) => {
                            emit_agent_complete(
                                sink.as_ref(),
                                &request.tab_id,
                                completion_outcome(outcome.suspended),
                            );
                            Ok(())
                        }
                        Err(error) => {
                            emit_cli_failure(
                                sink.as_ref(),
                                &request.tab_id,
                                "turn_loop_failed",
                                &error,
                            );
                            Ok(())
                        }
                    }
                })
            })
            .await;

            if let Err(error) = res {
                emit_cli_failure(sink.as_ref(), &args.tab_id, "repl_failed", &error);
                eprintln!("agent-runtime error: {error}");
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        RunMode::Command => ExitCode::FAILURE,
    }
}
