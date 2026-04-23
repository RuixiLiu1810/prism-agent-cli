use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use agent_core::{
    emit_agent_complete, emit_error, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnProfile, EventSink, StaticConfigProvider,
    ToolExecutorFn,
};
use crossterm::{
    cursor,
    event::{self, Event},
    execute, queue,
    style::Print,
    terminal::{self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::args::{Args, ToolMode};
use crate::config_model::ResolvedConfig;
use crate::status_snapshot::CliStatusSnapshot;
use crate::{tool_executor, turn_runner};

use super::event_bridge::{map_payload, ChannelEventSink, TuiRuntimeEvent};
use super::input::{apply_input_action, to_action, UiAction};
use super::renderer::render_frame;
use super::types::ViewUpdate;
use super::view_model::TuiViewModel;

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

fn draw_frame(vm: &TuiViewModel, args: &Args, resolved: &ResolvedConfig) -> Result<(), String> {
    let snapshot = CliStatusSnapshot::collect(
        &resolved.provider,
        &resolved.model,
        &args.project_path,
        &vm.session_id,
        &resolved.output,
    );
    let (width, height) = terminal::size().map_err(|e| format!("terminal size failed: {e}"))?;
    let lines = render_frame(&snapshot, vm, width, height);

    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )
    .map_err(|e| format!("clear frame failed: {e}"))?;

    for (index, line) in lines.iter().enumerate() {
        queue!(stdout, Print(line)).map_err(|e| format!("draw line failed: {e}"))?;
        if index + 1 < lines.len() {
            queue!(stdout, Print("\r\n")).map_err(|e| format!("draw newline failed: {e}"))?;
        }
    }
    stdout
        .flush()
        .map_err(|e| format!("flush frame failed: {e}"))?;
    Ok(())
}

pub async fn run_tui_shell(
    args: Args,
    resolved: ResolvedConfig,
    runtime_state: Arc<AgentRuntimeState>,
    tool_mode: ToolMode,
) -> Result<(), String> {
    let mut stdout = std::io::stdout();
    enable_raw_mode().map_err(|e| format!("enable_raw_mode failed: {e}"))?;
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| format!("enter alt screen failed: {e}"))?;

    let result = run_tui_loop(args, resolved, runtime_state, tool_mode).await;

    let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    let _ = disable_raw_mode();
    result
}

async fn run_tui_loop(
    args: Args,
    resolved: ResolvedConfig,
    runtime_state: Arc<AgentRuntimeState>,
    tool_mode: ToolMode,
) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TuiRuntimeEvent>();
    let sink: Arc<dyn EventSink> = Arc::new(ChannelEventSink::new(tx));
    let mut vm = TuiViewModel::new(format!("{}-session", args.tab_id));
    let mut active_turn: Option<tokio::task::JoinHandle<()>> = None;

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

    loop {
        while let Ok(runtime_event) = rx.try_recv() {
            match runtime_event {
                TuiRuntimeEvent::AgentEvent(envelope) => {
                    if let Some(update) = map_payload(&envelope.payload) {
                        vm.apply_update(update);
                    }
                }
                TuiRuntimeEvent::AgentComplete(done) => {
                    vm.apply_update(ViewUpdate::TurnOutcome(done.outcome));
                }
            }
        }

        if let Some(handle) = &active_turn {
            if handle.is_finished() {
                if let Some(done) = active_turn.take() {
                    let _ = done.await;
                }
            }
        }

        draw_frame(&vm, &args, &resolved)?;

        if event::poll(Duration::from_millis(16)).map_err(|e| format!("poll failed: {e}"))? {
            let Event::Key(key) = event::read().map_err(|e| format!("read failed: {e}"))? else {
                continue;
            };
            let action = to_action(key).unwrap_or(UiAction::Noop);
            match action {
                UiAction::Exit => break,
                UiAction::ClearScreen => {
                    vm.lines.clear();
                    vm.selected_line = 0;
                    vm.focus = super::types::UiFocus::Input;
                }
                _ => {
                    if let Some(prompt) = apply_input_action(&mut vm, action) {
                        if active_turn.is_some() {
                            vm.apply_update(ViewUpdate::Semantic {
                                text: "Agent is still working on the previous turn".to_string(),
                                detail: "wait for completion before sending another prompt"
                                    .to_string(),
                            });
                            continue;
                        }

                        vm.push_user_prompt(prompt.clone());
                        let request = build_request(
                            &args.project_path,
                            &args.tab_id,
                            &resolved.model,
                            prompt,
                            &vm.session_id,
                            tool_mode,
                        );
                        let sink_for_turn = Arc::clone(&sink);
                        let runtime_state_for_turn = Arc::clone(&runtime_state);
                        let tool_executor_for_turn = Arc::clone(&tool_executor);
                        let provider = static_provider_for(&resolved);

                        active_turn = Some(tokio::spawn(async move {
                            if tool_mode == ToolMode::Off
                                && turn_runner::request_requires_tools(&request)
                            {
                                emit_error(
                                    sink_for_turn.as_ref(),
                                    &request.tab_id,
                                    "tool_backend_unavailable",
                                    "This prompt requires tool execution, but tools are off."
                                        .to_string(),
                                );
                                emit_agent_complete(sink_for_turn.as_ref(), &request.tab_id, "error");
                                return;
                            }

                            match turn_runner::run_turn(
                                sink_for_turn.as_ref(),
                                &provider,
                                runtime_state_for_turn.as_ref(),
                                &request,
                                tool_executor_for_turn,
                            )
                            .await
                            {
                                Ok(outcome) => {
                                    let outcome_str = if outcome.suspended {
                                        "suspended"
                                    } else {
                                        "completed"
                                    };
                                    emit_agent_complete(
                                        sink_for_turn.as_ref(),
                                        &request.tab_id,
                                        outcome_str,
                                    );
                                }
                                Err(error) => {
                                    emit_error(
                                        sink_for_turn.as_ref(),
                                        &request.tab_id,
                                        "turn_loop_failed",
                                        error,
                                    );
                                    emit_agent_complete(
                                        sink_for_turn.as_ref(),
                                        &request.tab_id,
                                        "error",
                                    );
                                }
                            }
                        }));
                    }
                }
            }
        }
    }

    if let Some(handle) = active_turn {
        handle.abort();
        let _ = handle.await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_same_session_on_suspended_outcome() {
        let mut vm = TuiViewModel::new("tab-session".to_string());
        vm.apply_update(ViewUpdate::TurnOutcome("suspended".to_string()));
        assert!(vm.waiting_for_approval);
        assert_eq!(vm.session_id, "tab-session");
    }

    #[test]
    fn completes_turn_clears_waiting_flag() {
        let mut vm = TuiViewModel::new("tab-session".to_string());
        vm.apply_update(ViewUpdate::WaitingApproval("approve shell".to_string()));
        vm.apply_update(ViewUpdate::TurnOutcome("completed".to_string()));
        assert!(!vm.waiting_for_approval);
    }
}
