use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use agent_core::{
    emit_agent_complete, emit_error, AgentCompletePayload, AgentErrorEvent, AgentEventEnvelope,
    AgentEventPayload, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState, AgentTaskKind,
    AgentToolCallEvent, AgentToolResultEvent, AgentTurnDescriptor, AgentTurnProfile, EventSink,
    StaticConfigProvider, ToolExecutorFn,
};

use crate::args::{Args, ToolMode};
use crate::command_router::{self, ReplCommand};
use crate::config_model::ResolvedConfig;
use crate::repl;
use crate::status_snapshot::CliStatusSnapshot;
use crate::{tool_executor, turn_runner};

fn write_line<W: Write>(writer: &mut W, line: &str) {
    let _ = writer.write_all(line.as_bytes());
    let _ = writer.flush();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiSessionStatus {
    Idle,
    Busy,
    WaitingApproval,
}

struct StreamingTuiEventSink {
    writer: Mutex<Vec<u8>>,
    mirror_stdout: bool,
    session_status: Mutex<UiSessionStatus>,
}

impl StreamingTuiEventSink {
    fn stdout() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: true,
            session_status: Mutex::new(UiSessionStatus::Idle),
        }
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: false,
            session_status: Mutex::new(UiSessionStatus::Idle),
        }
    }

    #[cfg(test)]
    fn take_test_output(&self) -> String {
        let mut guard = match self.writer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let out = String::from_utf8_lossy(&guard).to_string();
        guard.clear();
        out
    }

    fn write_human(&self, line: &str) {
        if let Ok(mut guard) = self.writer.lock() {
            write_line(&mut *guard, line);
        }
        if self.mirror_stdout {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            write_line(&mut handle, line);
        }
    }

    fn set_status(&self, next: UiSessionStatus) {
        if let Ok(mut guard) = self.session_status.lock() {
            *guard = next;
        }
    }

    fn session_status(&self) -> UiSessionStatus {
        match self.session_status.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn prompt_prefix(&self) -> &'static str {
        match self.session_status() {
            UiSessionStatus::Idle => "> ",
            UiSessionStatus::Busy => "(busy)> ",
            UiSessionStatus::WaitingApproval => "(approval)> ",
        }
    }

    fn on_event_payload(&self, payload: &AgentEventPayload) {
        match payload {
            AgentEventPayload::Status(status) => {
                if status.stage == "awaiting_approval" {
                    self.set_status(UiSessionStatus::WaitingApproval);
                } else {
                    self.set_status(UiSessionStatus::Busy);
                }
            }
            AgentEventPayload::ToolResult(result) => {
                if result
                    .content
                    .get("approvalRequired")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                {
                    self.set_status(UiSessionStatus::WaitingApproval);
                }
            }
            AgentEventPayload::Error(_) => {
                self.set_status(UiSessionStatus::Idle);
            }
            _ => {}
        }
    }

    fn on_turn_complete(&self, outcome: &str) {
        if outcome == "suspended" {
            self.set_status(UiSessionStatus::WaitingApproval);
        } else {
            self.set_status(UiSessionStatus::Idle);
        }
    }

    fn render_tool_call(call: &AgentToolCallEvent) -> String {
        format!("\n[tool] {} ({})\n", call.tool_name, call.call_id)
    }

    fn render_tool_result(result: &AgentToolResultEvent) -> String {
        if result
            .content
            .get("approvalRequired")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return format!(
                "\n[semantic] {}\n[detail] run /approve shell once or /approve shell session\n",
                result.preview
            );
        }

        format!(
            "\n[semantic] {}\n[detail] tool={} call_id={} is_error={}\n",
            result.preview, result.tool_name, result.call_id, result.is_error
        )
    }

    fn render_error(error: &AgentErrorEvent) -> String {
        format!("\n[error:{}] {}\n", error.code, error.message)
    }
}

impl EventSink for StreamingTuiEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
        self.on_event_payload(&envelope.payload);

        let line = match &envelope.payload {
            AgentEventPayload::Status(status) => {
                format!("\n[{}] {}\n", status.stage, status.message)
            }
            AgentEventPayload::MessageDelta(delta) => delta.delta.clone(),
            AgentEventPayload::ToolCall(call) => Self::render_tool_call(call),
            AgentEventPayload::ToolResult(result) => Self::render_tool_result(result),
            AgentEventPayload::Error(error) => Self::render_error(error),
            _ => String::new(),
        };

        if !line.is_empty() {
            self.write_human(&line);
        }
    }

    fn emit_complete(&self, payload: &AgentCompletePayload) {
        self.on_turn_complete(&payload.outcome);
        self.write_human(&format!("\n[turn:{}]\n", payload.outcome));
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

fn render_help_panel() {
    println!(
        "TUI (streaming) commands:\n  /help\n  /commands\n  /status\n  /approve shell once|session|deny\n  exit|quit"
    );
}

fn render_commands_panel() {
    println!(
        "Supported commands:\n  /help\n  /commands\n  /status\n  /approve shell once|session|deny"
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

pub async fn run_tui_shell(
    args: Args,
    resolved: ResolvedConfig,
    runtime_state: Arc<AgentRuntimeState>,
    tool_mode: ToolMode,
) -> Result<(), String> {
    println!("[tui] streaming mode (non-fullscreen)");

    let streaming_sink = Arc::new(StreamingTuiEventSink::stdout());
    let sink: Arc<dyn EventSink> = streaming_sink.clone();
    let local_session_id = format!("{}-session", args.tab_id);

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

    let mut stdout = io::stdout();
    let reader = repl::stdin_reader();
    let repl_args = args.clone();
    let repl_resolved = resolved.clone();
    let repl_runtime_state = Arc::clone(&runtime_state);
    let repl_sink = Arc::clone(&sink);
    let repl_streaming_sink_for_prompt = Arc::clone(&streaming_sink);
    let repl_streaming_sink_for_submit = Arc::clone(&streaming_sink);
    let repl_tool_executor = Arc::clone(&tool_executor);

    let mut reader = reader;
    repl::run_repl_with_prompt(&mut reader, &mut stdout, move || {
        repl_streaming_sink_for_prompt.prompt_prefix().to_string()
    }, move |prompt| {
        match command_router::parse_repl_command(&prompt) {
            ReplCommand::Help => {
                render_help_panel();
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Commands => {
                render_commands_panel();
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Status => {
                let snapshot = CliStatusSnapshot::collect(
                    &repl_resolved.provider,
                    &repl_resolved.model,
                    &repl_args.project_path,
                    &local_session_id,
                    &repl_resolved.output,
                );
                render_status_inline(&snapshot);
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::ApproveShellOnce => {
                let runtime_state = Arc::clone(&repl_runtime_state);
                let tab_id = repl_args.tab_id.clone();
                let streaming_sink = Arc::clone(&repl_streaming_sink_for_submit);
                return Box::pin(async move {
                    match runtime_state
                        .set_tool_approval(&tab_id, "run_shell_command", "allow_once")
                        .await
                    {
                        Ok(()) => {
                            streaming_sink.set_status(UiSessionStatus::Idle);
                            println!("Approved shell for one command in this session.");
                        }
                        Err(err) => eprintln!("agent-runtime error: {}", err),
                    }
                    Ok(())
                });
            }
            ReplCommand::ApproveShellSession => {
                let runtime_state = Arc::clone(&repl_runtime_state);
                let tab_id = repl_args.tab_id.clone();
                let streaming_sink = Arc::clone(&repl_streaming_sink_for_submit);
                return Box::pin(async move {
                    match runtime_state
                        .set_tool_approval(&tab_id, "run_shell_command", "allow_session")
                        .await
                    {
                        Ok(()) => {
                            streaming_sink.set_status(UiSessionStatus::Idle);
                            println!("Approved shell for this session.");
                        }
                        Err(err) => eprintln!("agent-runtime error: {}", err),
                    }
                    Ok(())
                });
            }
            ReplCommand::ApproveShellDeny => {
                let runtime_state = Arc::clone(&repl_runtime_state);
                let tab_id = repl_args.tab_id.clone();
                let streaming_sink = Arc::clone(&repl_streaming_sink_for_submit);
                return Box::pin(async move {
                    match runtime_state
                        .set_tool_approval(&tab_id, "run_shell_command", "deny_session")
                        .await
                    {
                        Ok(()) => {
                            streaming_sink.set_status(UiSessionStatus::Idle);
                            println!("Denied shell for this session.");
                        }
                        Err(err) => eprintln!("agent-runtime error: {}", err),
                    }
                    Ok(())
                });
            }
            ReplCommand::Unknown { raw, suggestion } => {
                if let Some(suggestion) = suggestion {
                    println!("Unknown command: {}. Did you mean {}?", raw, suggestion);
                } else {
                    println!("Unknown command: {}", raw);
                }
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Config
            | ReplCommand::Clear
            | ReplCommand::ModelShow
            | ReplCommand::ModelSet(_) => {
                println!("Command is not available in streaming tui mode yet. Use --ui-mode classic.");
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::None => {}
        }

        let request = build_request(
            &repl_args.project_path,
            &repl_args.tab_id,
            &repl_resolved.model,
            prompt,
            &local_session_id,
            tool_mode,
        );
        let sink = Arc::clone(&repl_sink);
        let runtime_state = Arc::clone(&repl_runtime_state);
        let tool_executor = Arc::clone(&repl_tool_executor);
        let config_provider = static_provider_for(&repl_resolved);

        Box::pin(async move {
            if tool_mode == ToolMode::Off && turn_runner::request_requires_tools(&request) {
                let message = "This prompt requires tool execution, but tools are off.".to_string();
                emit_error(
                    sink.as_ref(),
                    &request.tab_id,
                    "tool_backend_unavailable",
                    message,
                );
                emit_agent_complete(sink.as_ref(), &request.tab_id, "error");
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
                    let outcome_str = if outcome.suspended {
                        "suspended"
                    } else {
                        "completed"
                    };
                    emit_agent_complete(sink.as_ref(), &request.tab_id, outcome_str);
                }
                Err(error) => {
                    emit_error(sink.as_ref(), &request.tab_id, "turn_loop_failed", error);
                    emit_agent_complete(sink.as_ref(), &request.tab_id, "error");
                }
            }
            Ok(())
        })
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{AgentMessageDeltaEvent, AgentStatusEvent};

    #[test]
    fn stream_sink_preserves_message_delta_whitespace() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::MessageDelta(AgentMessageDeltaEvent {
                delta: " hello world ".to_string(),
            }),
        });
        let out = sink.take_test_output();
        assert!(out.contains(" hello world "));
    }

    #[test]
    fn stream_sink_emits_semantic_tool_lines_and_approval_hint() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::ToolResult(AgentToolResultEvent {
                tool_name: "run_shell_command".to_string(),
                call_id: "call-1".to_string(),
                is_error: true,
                preview: "run_shell_command requires approval".to_string(),
                content: serde_json::json!({"approvalRequired": true}),
                display: serde_json::Value::Null,
            }),
        });
        let out = sink.take_test_output();
        assert!(out.contains("[semantic] run_shell_command requires approval"));
        assert!(out.contains("/approve shell once"));
    }

    #[test]
    fn stream_sink_formats_status_lines() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "streaming".to_string(),
                message: "connected".to_string(),
            }),
        });
        let out = sink.take_test_output();
        assert!(out.contains("[streaming] connected"));
    }

    #[test]
    fn status_moves_to_waiting_on_approval_and_suspended_complete() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::ToolResult(AgentToolResultEvent {
                tool_name: "run_shell_command".to_string(),
                call_id: "call-2".to_string(),
                is_error: true,
                preview: "run_shell_command requires approval".to_string(),
                content: serde_json::json!({"approvalRequired": true}),
                display: serde_json::Value::Null,
            }),
        });
        assert_eq!(sink.session_status(), UiSessionStatus::WaitingApproval);
        assert_eq!(sink.prompt_prefix(), "(approval)> ");

        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "suspended".to_string(),
        });
        assert_eq!(sink.session_status(), UiSessionStatus::WaitingApproval);
        assert_eq!(sink.prompt_prefix(), "(approval)> ");
    }

    #[test]
    fn status_returns_to_idle_after_non_suspended_complete() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "streaming".to_string(),
                message: "connected".to_string(),
            }),
        });
        assert_eq!(sink.session_status(), UiSessionStatus::Busy);

        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "completed".to_string(),
        });
        assert_eq!(sink.session_status(), UiSessionStatus::Idle);
        assert_eq!(sink.prompt_prefix(), "> ");
    }
}
