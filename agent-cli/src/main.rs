mod args;
mod command_router;
mod config_commands;
mod config_model;
mod config_resolver;
mod config_store;
mod config_wizard;
mod header_renderer;
mod local_tools;
mod output;
mod repl;
mod status_snapshot;
mod tool_executor;
mod turn_runner;

use std::{process::ExitCode, sync::Arc};

use agent_core::{
    emit_agent_complete, emit_error, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnProfile, EventSink, StaticConfigProvider,
    ToolExecutorFn,
};
use args::{Args, Command, ConfigSubcommand, OutputMode, RunMode, ToolMode};
use clap::Parser;
use config_model::ResolvedConfig;
use output::{HumanEventSink, JsonlEventSink};
use status_snapshot::CliStatusSnapshot;

fn emit_cli_failure(sink: &dyn EventSink, tab_id: &str, code: &str, message: &str) {
    emit_error(sink, tab_id, code, message.to_string());
    emit_agent_complete(sink, tab_id, "error");
}

fn completion_outcome(suspended: bool) -> &'static str {
    if suspended {
        "suspended"
    } else {
        "completed"
    }
}

fn build_request(
    project_path: &str,
    tab_id: &str,
    model: &str,
    prompt: String,
    local_session_id: &str,
    tool_mode: ToolMode,
) -> AgentTurnDescriptor {
    let mut request = AgentTurnDescriptor {
        project_path: project_path.to_string(),
        prompt,
        tab_id: tab_id.to_string(),
        model: Some(model.to_string()),
        local_session_id: Some(local_session_id.to_string()),
        previous_response_id: None,
        turn_profile: None,
    };

    if tool_mode == ToolMode::Off {
        request.turn_profile = Some(AgentTurnProfile {
            task_kind: AgentTaskKind::SuggestionOnly,
            response_mode: AgentResponseMode::SuggestionOnly,
            ..AgentTurnProfile::default()
        });
    }

    request
}

fn static_provider_for(resolved: &ResolvedConfig) -> StaticConfigProvider {
    let mut config = AgentRuntimeConfig::default_local_agent();
    config.provider = resolved.provider.clone();
    config.model = resolved.model.clone();
    config.api_key = Some(resolved.api_key.clone());
    config.base_url = resolved.base_url.clone();

    StaticConfigProvider {
        config,
        config_dir: std::env::temp_dir().join("agent-runtime"),
    }
}

fn render_header(
    output_mode: OutputMode,
    args: &Args,
    resolved: &ResolvedConfig,
    local_session_id: &str,
) -> Result<(), String> {
    if output_mode != OutputMode::Human {
        return Ok(());
    }

    let snapshot = CliStatusSnapshot::collect(
        &resolved.provider,
        &resolved.model,
        &args.project_path,
        local_session_id,
        &resolved.output,
    );

    let mut stdout = std::io::stdout();
    header_renderer::print_header(&mut stdout, &snapshot)
}

fn clear_and_render_header(
    output_mode: OutputMode,
    args: &Args,
    resolved: &ResolvedConfig,
    local_session_id: &str,
) -> Result<(), String> {
    if output_mode != OutputMode::Human {
        return Ok(());
    }

    let mut stdout = std::io::stdout();
    header_renderer::clear_screen(&mut stdout)?;
    render_header(output_mode, args, resolved, local_session_id)
}

fn render_help_panel() {
    println!(
        "Agent CLI commands:
  /help      Show quick help
  /commands  Show the full command list
  /config    Edit local config interactively
  /model     Show current model
  /model X   Persist model X to local config file
  /approve shell once|session|deny  Control shell tool approval
  /status    Show current runtime status
  /clear     Clear the screen and redraw header
  exit|quit  Leave REPL"
    );
}

fn render_commands_panel() {
    println!(
        "Supported commands:
  /help
  /commands
  /config
  /model
  /model <model-name>
  /approve shell once|session|deny
  /status
  /clear"
    );
}

fn render_status_inline(snapshot: &CliStatusSnapshot) {
    let dirty_suffix = if snapshot.git_dirty { "*" } else { "" };
    println!(
        "provider/model: {}/{} | output: {} | session: {}",
        snapshot.provider, snapshot.model, snapshot.output_mode, snapshot.session_id
    );
    println!(
        "project: {} | git: {}{}",
        snapshot.project_path, snapshot.git_branch, dirty_suffix
    );
}

fn persist_model_to_local_config(model: &str) -> Result<String, String> {
    let path = config_store::default_config_path()?;
    let mut cfg = config_store::load_config(&path)?.unwrap_or_default();
    cfg.model = Some(model.trim().to_string());
    config_store::save_config_atomic(&path, &cfg)?;
    Ok(format!(
        "Model updated in local config: {} -> {}",
        model.trim(),
        path.display()
    ))
}

fn model_override_hint(args: &Args) -> Option<&'static str> {
    if args
        .model
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Some("Note: --model is set in CLI args and still takes precedence.");
    }
    if std::env::var("AGENT_MODEL")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Some("Note: AGENT_MODEL is set and still takes precedence.");
    }
    None
}

fn resolve_effective_config(args: &Args, allow_wizard: bool) -> Result<ResolvedConfig, String> {
    let path = config_store::default_config_path()?;
    let file_cfg = match config_store::load_config(&path) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("config warning: {}", err);
            None
        }
    };

    let cli_raw = config_resolver::RawConfig {
        provider: args.provider.clone(),
        model: args.model.clone(),
        api_key: args.api_key.clone(),
        base_url: args.base_url.clone(),
        output: args.output.clone(),
    };

    let env_raw = config_resolver::RawConfig {
        provider: std::env::var("AGENT_PROVIDER").ok(),
        model: std::env::var("AGENT_MODEL").ok(),
        api_key: std::env::var("AGENT_API_KEY").ok(),
        base_url: std::env::var("AGENT_BASE_URL").ok(),
        output: std::env::var("AGENT_OUTPUT").ok(),
    };

    let mut merged = config_resolver::merge_sources(
        &cli_raw,
        &env_raw,
        &config_resolver::file_to_raw(file_cfg.clone()),
    );

    if !config_resolver::detect_missing(&merged).is_empty() {
        if !allow_wizard {
            return Err(
                "required config missing: provider/model/api_key. Run `agent-runtime config init`"
                    .to_string(),
            );
        }

        let mut io = config_wizard::StdioWizardIo;
        let wizard_cfg = config_wizard::run_wizard(&mut io, file_cfg.as_ref())?;
        config_store::save_config_atomic(&path, &wizard_cfg)?;
        merged = config_resolver::merge_sources(
            &cli_raw,
            &env_raw,
            &config_resolver::file_to_raw(Some(wizard_cfg)),
        );
    }

    config_resolver::finalize(merged)
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
    fn build_request_sets_suggestion_profile_in_off_mode() {
        let request = build_request(
            ".",
            "tab-1",
            "MiniMax-M1",
            "hello".to_string(),
            "session-1",
            ToolMode::Off,
        );
        let profile = request
            .turn_profile
            .unwrap_or_else(|| panic!("turn_profile should be set in off mode"));
        assert_eq!(profile.task_kind, AgentTaskKind::SuggestionOnly);
        assert_eq!(profile.response_mode, AgentResponseMode::SuggestionOnly);
    }

    #[test]
    fn build_request_keeps_default_profile_in_safe_mode() {
        let request = build_request(
            ".",
            "tab-1",
            "MiniMax-M1",
            "hello".to_string(),
            "session-1",
            ToolMode::Safe,
        );
        assert!(request.turn_profile.is_none());
    }

    #[test]
    fn startup_requests_wizard_when_required_fields_are_missing() {
        let merged = config_resolver::RawConfig::default();
        let missing = config_resolver::detect_missing(&merged);
        assert_eq!(missing.len(), 3);
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    if let RunMode::Command = args.run_mode() {
        if let Some(Command::Config { action }) = &args.command {
            let mut io = config_wizard::StdioWizardIo;
            match config_commands::execute_config_command(action, &mut io) {
                Ok(msg) => {
                    println!("{}", msg);
                    return ExitCode::SUCCESS;
                }
                Err(err) => {
                    eprintln!("agent-runtime error: {}", err);
                    return ExitCode::FAILURE;
                }
            }
        }

        eprintln!("agent-runtime error: unsupported command");
        return ExitCode::FAILURE;
    }

    let resolved = match resolve_effective_config(&args, true) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("agent-runtime error: {}", err);
            return ExitCode::FAILURE;
        }
    };

    let output_mode = match args::parse_output_mode(&resolved.output) {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("agent-runtime error: {}", err);
            return ExitCode::FAILURE;
        }
    };
    let tool_mode = match args::parse_tool_mode(args.tool_mode.as_deref().unwrap_or("safe")) {
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

    let runtime_state = Arc::new(AgentRuntimeState::default());

    let tool_runtime_state = Arc::clone(&runtime_state);
    let tool_tab_id = args.tab_id.clone();
    let tool_project_path = args.project_path.clone();
    let tool_executor: ToolExecutorFn = Arc::new(move |call, cancel_rx| {
        let runtime_state = Arc::clone(&tool_runtime_state);
        let tab_id = tool_tab_id.clone();
        let project_root = tool_project_path.clone();
        Box::pin(async move {
            tool_executor::execute_cli_tool(runtime_state, tab_id, project_root, call, cancel_rx)
                .await
        })
    });

    let local_session_id = format!("{}-session", args.tab_id);

    match args.run_mode() {
        RunMode::SingleTurn => {
            let prompt = args.prompt.clone().unwrap_or_default();
            let request = build_request(
                &args.project_path,
                &args.tab_id,
                &resolved.model,
                prompt,
                &local_session_id,
                tool_mode,
            );

            if tool_mode == ToolMode::Off && turn_runner::request_requires_tools(&request) {
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

            let config_provider = static_provider_for(&resolved);
            match turn_runner::run_turn(
                sink.as_ref(),
                &config_provider,
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
            if let Err(err) = render_header(output_mode, &args, &resolved, &local_session_id) {
                eprintln!("agent-runtime warning: {}", err);
            }

            let mut stdout = std::io::stdout();
            let reader = repl::stdin_reader();
            let repl_args = args.clone();
            let repl_session_id = local_session_id.clone();
            let repl_sink = Arc::clone(&sink);
            let repl_runtime_state = Arc::clone(&runtime_state);
            let repl_tool_executor = Arc::clone(&tool_executor);
            let repl_output_mode = output_mode;

            let res = repl::run_repl(reader, &mut stdout, move |prompt| {
                match command_router::parse_repl_command(&prompt) {
                    command_router::ReplCommand::Config => {
                        let mut io = config_wizard::StdioWizardIo;
                        match config_commands::execute_config_command(&ConfigSubcommand::Edit, &mut io)
                        {
                            Ok(message) => {
                                println!("{}", message);
                                match resolve_effective_config(&repl_args, false) {
                                    Ok(updated) => {
                                        let _ = render_header(
                                            repl_output_mode,
                                            &repl_args,
                                            &updated,
                                            &repl_session_id,
                                        );
                                    }
                                    Err(err) => eprintln!("agent-runtime error: {}", err),
                                }
                            }
                            Err(err) => eprintln!("agent-runtime error: {}", err),
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::Help => {
                        render_help_panel();
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::Commands => {
                        render_commands_panel();
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::Status => {
                        match resolve_effective_config(&repl_args, false) {
                            Ok(current) => {
                                let snapshot = CliStatusSnapshot::collect(
                                    &current.provider,
                                    &current.model,
                                    &repl_args.project_path,
                                    &repl_session_id,
                                    &current.output,
                                );
                                render_status_inline(&snapshot);
                            }
                            Err(err) => eprintln!("agent-runtime error: {}", err),
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::Clear => {
                        match resolve_effective_config(&repl_args, false) {
                            Ok(current) => {
                                if let Err(err) = clear_and_render_header(
                                    repl_output_mode,
                                    &repl_args,
                                    &current,
                                    &repl_session_id,
                                ) {
                                    eprintln!("agent-runtime error: {}", err);
                                }
                            }
                            Err(err) => eprintln!("agent-runtime error: {}", err),
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::ModelShow => {
                        match resolve_effective_config(&repl_args, false) {
                            Ok(current) => println!("Current model: {}", current.model),
                            Err(err) => eprintln!("agent-runtime error: {}", err),
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::ModelSet(model) => {
                        match persist_model_to_local_config(&model) {
                            Ok(message) => {
                                println!("{}", message);
                                if let Some(hint) = model_override_hint(&repl_args) {
                                    println!("{}", hint);
                                }
                                match resolve_effective_config(&repl_args, false) {
                                    Ok(current) => {
                                        let _ = render_header(
                                            repl_output_mode,
                                            &repl_args,
                                            &current,
                                            &repl_session_id,
                                        );
                                    }
                                    Err(err) => eprintln!("agent-runtime error: {}", err),
                                }
                            }
                            Err(err) => eprintln!("agent-runtime error: {}", err),
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::ApproveShellOnce => {
                        let runtime_state = Arc::clone(&repl_runtime_state);
                        let tab_id = repl_args.tab_id.clone();
                        return Box::pin(async move {
                            match runtime_state
                                .set_tool_approval(&tab_id, "run_shell_command", "allow_once")
                                .await
                            {
                                Ok(()) => {
                                    println!("Approved shell for one command in this session.")
                                }
                                Err(err) => eprintln!("agent-runtime error: {}", err),
                            }
                            Ok(())
                        });
                    }
                    command_router::ReplCommand::ApproveShellSession => {
                        let runtime_state = Arc::clone(&repl_runtime_state);
                        let tab_id = repl_args.tab_id.clone();
                        return Box::pin(async move {
                            match runtime_state
                                .set_tool_approval(&tab_id, "run_shell_command", "allow_session")
                                .await
                            {
                                Ok(()) => println!("Approved shell for this session."),
                                Err(err) => eprintln!("agent-runtime error: {}", err),
                            }
                            Ok(())
                        });
                    }
                    command_router::ReplCommand::ApproveShellDeny => {
                        let runtime_state = Arc::clone(&repl_runtime_state);
                        let tab_id = repl_args.tab_id.clone();
                        return Box::pin(async move {
                            match runtime_state
                                .set_tool_approval(&tab_id, "run_shell_command", "deny_session")
                                .await
                            {
                                Ok(()) => println!("Denied shell for this session."),
                                Err(err) => eprintln!("agent-runtime error: {}", err),
                            }
                            Ok(())
                        });
                    }
                    command_router::ReplCommand::Unknown { raw, suggestion } => {
                        if let Some(suggestion) = suggestion {
                            println!("Unknown command: {}. Did you mean {}?", raw, suggestion);
                        } else {
                            println!("Unknown command: {}", raw);
                        }
                        return Box::pin(async { Ok(()) });
                    }
                    command_router::ReplCommand::None => {}
                }

                let resolved = match resolve_effective_config(&repl_args, true) {
                    Ok(cfg) => cfg,
                    Err(err) => {
                        emit_cli_failure(
                            repl_sink.as_ref(),
                            &repl_args.tab_id,
                            "config_resolve_failed",
                            &err,
                        );
                        return Box::pin(async { Ok(()) });
                    }
                };

                let request = build_request(
                    &repl_args.project_path,
                    &repl_args.tab_id,
                    &resolved.model,
                    prompt,
                    &repl_session_id,
                    tool_mode,
                );
                let sink = Arc::clone(&repl_sink);
                let runtime_state = Arc::clone(&repl_runtime_state);
                let tool_executor = Arc::clone(&repl_tool_executor);
                let config_provider = static_provider_for(&resolved);
                let args_for_turn = repl_args.clone();
                let session_id_for_turn = repl_session_id.clone();

                Box::pin(async move {
                    if tool_mode == ToolMode::Off && turn_runner::request_requires_tools(&request)
                    {
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
                        &config_provider,
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
                            if let Ok(current) = resolve_effective_config(&args_for_turn, false) {
                                let _ = render_header(
                                    repl_output_mode,
                                    &args_for_turn,
                                    &current,
                                    &session_id_for_turn,
                                );
                            }
                            Ok(())
                        }
                        Err(error) => {
                            emit_cli_failure(
                                sink.as_ref(),
                                &request.tab_id,
                                "turn_loop_failed",
                                &error,
                            );
                            if let Ok(current) = resolve_effective_config(&args_for_turn, false) {
                                let _ = render_header(
                                    repl_output_mode,
                                    &args_for_turn,
                                    &current,
                                    &session_id_for_turn,
                                );
                            }
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
