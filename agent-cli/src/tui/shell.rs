use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use agent_core::{
    emit_agent_complete, emit_error, AgentCompletePayload, AgentErrorEvent, AgentEventEnvelope,
    AgentEventPayload, AgentResponseMode, AgentRuntimeConfig, AgentRuntimeState, AgentTaskKind,
    AgentToolResultEvent, AgentTurnDescriptor, AgentTurnProfile, EventSink,
    StaticConfigProvider, ToolExecutorFn,
};

use crate::args::{Args, ToolMode};
use crate::command_router::{self, ReplCommand};
use crate::config_model::ResolvedConfig;
use crate::repl;
use crate::status_snapshot::CliStatusSnapshot;
use crate::tui::icons::{reduced_motion_enabled, Icons};
use crate::tui::layout::{render_header_block, render_notice_line, render_slots, Slot, SlotLine};
use crate::tui::suggestions::render_command_suggestions;
use crate::tui::theme::{Role, Theme};
use crate::tui::transcript::render_user_command_rows;
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct SessionChromeState {
    rendered_for: Option<String>,
}

impl SessionChromeState {
    fn should_render(&self, session_id: &str) -> bool {
        self.rendered_for.as_deref() != Some(session_id)
    }

    fn mark_rendered(&mut self, session_id: &str) {
        self.rendered_for = Some(session_id.to_string());
    }
}

struct StreamingTuiEventSink {
    writer: Mutex<Vec<u8>>,
    mirror_stdout: bool,
    fixed_input_bar: bool,
    session_status: Mutex<UiSessionStatus>,
    icons: Icons,
    theme: Theme,
    reduced_motion: bool,
    spinner_tick: AtomicUsize,
    assistant_prefix_printed: Mutex<bool>,
    assistant_col: Mutex<usize>,
}

impl StreamingTuiEventSink {
    fn stdout() -> Self {
        let fixed_input_bar = io::stdout().is_terminal();
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: true,
            fixed_input_bar,
            session_status: Mutex::new(UiSessionStatus::Idle),
            icons: Icons::detect(),
            theme: Theme::detect(),
            reduced_motion: reduced_motion_enabled(),
            spinner_tick: AtomicUsize::new(0),
            assistant_prefix_printed: Mutex::new(false),
            assistant_col: Mutex::new(0),
        }
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: false,
            fixed_input_bar: false,
            session_status: Mutex::new(UiSessionStatus::Idle),
            icons: Icons::detect(),
            theme: Theme { enable_color: false },
            reduced_motion: true,
            spinner_tick: AtomicUsize::new(0),
            assistant_prefix_printed: Mutex::new(false),
            assistant_col: Mutex::new(0),
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

    fn terminal_width_or_default() -> usize {
        crossterm::terminal::size()
            .map(|(width, _)| width as usize)
            .unwrap_or(120)
    }

    fn fixed_bar_layout(&self) -> Option<(usize, u16, u16, u16, u16)> {
        if !(self.fixed_input_bar && self.mirror_stdout) {
            return None;
        }
        let (width, height) = crossterm::terminal::size().ok()?;
        // Keep enough vertical room for header + notice + transcript area; otherwise
        // a pinned bar can overwrite startup chrome (including the notice line).
        if width < 8 || height < 12 {
            return None;
        }
        let width = (width as usize).saturating_sub(1).max(1);
        let scroll_bottom = height.saturating_sub(3).max(1);
        let top_row = height.saturating_sub(2);
        let input_row = height.saturating_sub(1);
        let bottom_row = height;
        Some((width, scroll_bottom, top_row, input_row, bottom_row))
    }

    fn draw_fixed_input_bar(&self) {
        if let Some((width, scroll_bottom, top_row, input_row, bottom_row)) = self.fixed_bar_layout() {
            let border = self.theme.paint(Role::Subtle, "─".repeat(width));
            self.write_human(&format!(
                "\x1b[1;{scroll_bottom}r\x1b[{top_row};1H\x1b[2K{border}\x1b[{input_row};1H\x1b[2K› \x1b[{bottom_row};1H\x1b[2K{border}\x1b[{input_row};3H"
            ));
        }
    }

    fn reset_assistant_stream(&self) {
        if let Ok(mut prefix) = self.assistant_prefix_printed.lock() {
            *prefix = false;
        }
        if let Ok(mut col) = self.assistant_col.lock() {
            *col = 0;
        }
    }

    fn flush_assistant_stream_boundary(&self) {
        let should_flush = match self.assistant_prefix_printed.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        };
        if should_flush {
            self.write_human("\n");
            self.reset_assistant_stream();
        }
    }

    fn write_assistant_delta_wrapped(&self, delta: &str) {
        let width = Self::terminal_width_or_default().max(4);
        let mut prefix_guard = match self.assistant_prefix_printed.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut col_guard = match self.assistant_col.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let mut normalized = if !*prefix_guard {
            delta.trim_start_matches(['\n', '\r']).to_string()
        } else {
            delta.to_string()
        };
        normalized = normalize_markdown_chunk(&normalized);
        if normalized.is_empty() {
            return;
        }

        if !*prefix_guard {
            self.write_human("● ");
            *prefix_guard = true;
            *col_guard = 2;
        }

        for ch in normalized.chars() {
            if ch == '\n' {
                self.write_human("\n  ");
                *col_guard = 2;
                continue;
            }
            if *col_guard >= width {
                self.write_human("\n  ");
                *col_guard = 2;
            }
            let mut buf = [0; 4];
            self.write_human(ch.encode_utf8(&mut buf));
            *col_guard += 1;
        }
    }

    fn render_user_prompt(&self, prompt: &str) {
        self.flush_assistant_stream_boundary();
        for row in render_user_command_rows(&self.theme, prompt, Self::terminal_width_or_default()) {
            self.write_human(&(row + "\n"));
        }
        // Keep a clear rhythm between user command rows and assistant response rows.
        self.write_human("\n");
    }

    fn render_input_frame_prompt(&self) -> String {
        if self.fixed_bar_layout().is_some() {
            self.draw_fixed_input_bar();
            String::new()
        } else {
            self.prompt_prefix().to_string()
        }
    }

    fn prepare_transcript_output(&self) {
        if let Some((_, scroll_bottom, _, input_row, _)) = self.fixed_bar_layout() {
            self.write_human(&format!("\x1b[{input_row};1H\x1b[2K\x1b[{scroll_bottom};1H"));
        } else if self.mirror_stdout && io::stdout().is_terminal() {
            // Fallback for non-fixed-bar terminals: clear prompt echo line.
            self.write_human("\x1b[1A\r\x1b[2K");
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
        let _ = self.session_status();
        "› "
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

    fn render_tool_result(&self, result: &AgentToolResultEvent) -> Vec<SlotLine> {
        if result
            .content
            .get("approvalRequired")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return vec![
                SlotLine::new(
                    Slot::Scrollable,
                    self.theme.paint(
                        Role::Warning,
                        format!("{} {}", self.icons.waiting, result.preview),
                    ),
                ),
                SlotLine::new(
                    Slot::Bottom,
                    self.theme.paint(
                        Role::Subtle,
                        format!(
                            "{} run /approve shell once or /approve shell session",
                            self.icons.detail
                        ),
                    ),
                ),
            ];
        }

        vec![
            SlotLine::new(
                Slot::Scrollable,
                self.theme.paint(
                    Role::Text,
                    format!("{} {}", self.icons.semantic, result.preview),
                ),
            ),
            SlotLine::new(
                Slot::Bottom,
                self.theme.paint(
                    Role::Subtle,
                    format!(
                        "{} tool={} call_id={} is_error={}",
                        self.icons.detail, result.tool_name, result.call_id, result.is_error
                    ),
                ),
            ),
        ]
    }

    fn render_error(&self, error: &AgentErrorEvent) -> SlotLine {
        SlotLine::new(
            Slot::Scrollable,
            self.theme.paint(
                Role::Error,
                format!("{} [{}] {}", self.icons.error, error.code, error.message),
            ),
        )
    }

    fn render_status(&self, stage: &str, message: &str) -> SlotLine {
        let role = if stage == "awaiting_approval" {
            Role::Warning
        } else {
            Role::Subtle
        };
        let prefix = if matches!(stage, "streaming" | "responding_after_tools") {
            let tick = self.spinner_tick.fetch_add(1, Ordering::Relaxed);
            self.icons.spinner_frame(tick, self.reduced_motion)
        } else {
            self.icons.semantic
        };

        SlotLine::new(
            Slot::Scrollable,
            self.theme
                .paint(role, format!("{} [{}] {}", prefix, stage, message)),
        )
    }
}

impl EventSink for StreamingTuiEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
        self.on_event_payload(&envelope.payload);

        if let AgentEventPayload::MessageDelta(delta) = &envelope.payload {
            self.write_assistant_delta_wrapped(&delta.delta);
            return;
        }

        self.flush_assistant_stream_boundary();

        let lines = match &envelope.payload {
            AgentEventPayload::Status(status) if status.stage == "awaiting_approval" => {
                vec![self.render_status(&status.stage, &status.message)]
            }
            AgentEventPayload::ToolResult(result)
                if result
                    .content
                    .get("approvalRequired")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false) =>
            {
                self.render_tool_result(result)
            }
            AgentEventPayload::Error(error) => vec![self.render_error(error)],
            _ => Vec::new(),
        };

        if !lines.is_empty() {
            self.write_human(&render_slots(&lines, None));
        }
    }

    fn emit_complete(&self, payload: &AgentCompletePayload) {
        self.flush_assistant_stream_boundary();
        self.on_turn_complete(&payload.outcome);
    }
}

fn normalize_markdown_chunk(delta: &str) -> String {
    fn strip_heading_prefix(line: &str) -> &str {
        let trimmed = line.trim_start_matches(' ');
        for prefix in ["### ", "## ", "# "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                return rest;
            }
        }
        line
    }

    let mut out = String::with_capacity(delta.len());
    for segment in delta.split_inclusive('\n') {
        let (line, newline) = if let Some(stripped) = segment.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (segment, "")
        };
        out.push_str(strip_heading_prefix(line));
        out.push_str(newline);
    }
    out.replace("**", "")
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

fn startup_notice_text() -> String {
    render_notice_line("Session ready", "/commands for help")
}

fn display_project_path(project_path: &str) -> String {
    let raw = project_path.trim();
    let mut path = if raw.is_empty() || raw == "." {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(raw)
    };
    if path.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            path = cwd.join(path);
        }
    }
    let display = path.to_string_lossy().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if display.starts_with(&home) {
            let suffix = &display[home.len()..];
            return format!("~{}", suffix);
        }
    }
    display
}

pub async fn run_tui_shell(
    args: Args,
    resolved: ResolvedConfig,
    runtime_state: Arc<AgentRuntimeState>,
    tool_mode: ToolMode,
) -> Result<(), String> {
    let streaming_sink = Arc::new(StreamingTuiEventSink::stdout());
    let sink: Arc<dyn EventSink> = streaming_sink.clone();
    let local_session_id = format!("{}-session", args.tab_id);
    let mut chrome_state = SessionChromeState::default();

    if chrome_state.should_render(&local_session_id) {
        let tool_mode_label = match tool_mode {
            ToolMode::Off => "tools off",
            ToolMode::Safe => "tools safe",
        };
        let model_line = format!("{} · {}", resolved.model, tool_mode_label);
        for line in render_header_block(
            "Claude Prism",
            &format!("v{}", env!("CARGO_PKG_VERSION")),
            &model_line,
            &display_project_path(&args.project_path),
        ) {
            streaming_sink.write_human(&(line + "\n"));
        }
        streaming_sink.write_human("\n");
        let notice = startup_notice_text();
        streaming_sink.write_human(&(notice + "\n\n"));
        chrome_state.mark_rendered(&local_session_id);
    }

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
    let repl_result = repl::run_repl_with_prompt(&mut reader, &mut stdout, move || {
        repl_streaming_sink_for_prompt.render_input_frame_prompt()
    }, move |prompt| {
        match command_router::parse_repl_command(&prompt) {
            ReplCommand::None => {
                repl_streaming_sink_for_submit.prepare_transcript_output();
                repl_streaming_sink_for_submit.render_user_prompt(&prompt);
            }
            ReplCommand::Help => {
                repl_streaming_sink_for_submit.prepare_transcript_output();
                render_help_panel();
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Commands => {
                repl_streaming_sink_for_submit.prepare_transcript_output();
                render_commands_panel();
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Status => {
                repl_streaming_sink_for_submit.prepare_transcript_output();
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
                repl_streaming_sink_for_submit.prepare_transcript_output();
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
                repl_streaming_sink_for_submit.prepare_transcript_output();
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
                repl_streaming_sink_for_submit.prepare_transcript_output();
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
                repl_streaming_sink_for_submit.prepare_transcript_output();
                if let Some(suggestion) = suggestion {
                    println!("Unknown command: {}. Did you mean {}?", raw, suggestion);
                } else {
                    println!("Unknown command: {}", raw);
                }
                if let Some(panel) = render_command_suggestions(&raw, 3) {
                    println!("{}", panel);
                }
                return Box::pin(async { Ok(()) });
            }
            ReplCommand::Config
            | ReplCommand::Clear
            | ReplCommand::ModelShow
            | ReplCommand::ModelSet(_) => {
                repl_streaming_sink_for_submit.prepare_transcript_output();
                println!("Command is not available in streaming tui mode yet. Use --ui-mode classic.");
                return Box::pin(async { Ok(()) });
            }
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
    }).await;

    if streaming_sink.fixed_bar_layout().is_some() {
        streaming_sink.write_human("\x1b[r\n");
    }

    repl_result
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
    fn submitted_prompt_is_rendered_as_user_command_row() {
        let sink = StreamingTuiEventSink::for_test();
        sink.render_user_prompt("who are you");
        let out = sink.take_test_output();
        assert!(out.contains("› who are you"));
    }

    #[test]
    fn message_delta_is_rendered_with_assistant_marker() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::MessageDelta(AgentMessageDeltaEvent {
                delta: "hello there".to_string(),
            }),
        });
        let out = sink.take_test_output();
        assert!(out.contains("● "));
    }

    #[test]
    fn notice_does_not_advertise_unavailable_model_command() {
        let text = startup_notice_text();
        assert!(!text.contains("/model"));
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
        assert!(out.contains("run_shell_command requires approval"));
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
        assert!(out.is_empty());
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
        assert_eq!(sink.prompt_prefix(), "› ");

        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "suspended".to_string(),
        });
        assert_eq!(sink.session_status(), UiSessionStatus::WaitingApproval);
        assert_eq!(sink.prompt_prefix(), "› ");
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
        assert_eq!(sink.prompt_prefix(), "› ");
    }

    #[test]
    fn suspended_then_completed_turns_stay_in_single_timeline() {
        let sink = StreamingTuiEventSink::for_test();

        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "awaiting_approval".to_string(),
                message: "waiting for shell approval".to_string(),
            }),
        });
        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "suspended".to_string(),
        });

        sink.set_status(UiSessionStatus::Idle);
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "streaming".to_string(),
                message: "resumed".to_string(),
            }),
        });
        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "completed".to_string(),
        });

        let out = sink.take_test_output();
        assert!(!out.contains("[turn:suspended]"));
        assert!(!out.contains("[turn:completed]"));
    }

    #[test]
    fn first_delta_strips_leading_newlines_before_marker() {
        let sink = StreamingTuiEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::MessageDelta(AgentMessageDeltaEvent {
                delta: "\n\nHello".to_string(),
            }),
        });
        let out = sink.take_test_output();
        assert!(out.starts_with("● Hello"));
    }

    #[test]
    fn prompt_frame_falls_back_to_plain_prefix_without_tty() {
        let sink = StreamingTuiEventSink::for_test();
        let prompt = sink.render_input_frame_prompt();
        assert_eq!(prompt, "› ");
    }

    #[test]
    fn display_project_path_resolves_dot_to_directory_like_path() {
        let path = super::display_project_path(".");
        assert!(path != ".");
    }

    #[test]
    fn header_renders_once_for_same_session() {
        let mut chrome = SessionChromeState::default();
        assert!(chrome.should_render("tab-a-session"));
        chrome.mark_rendered("tab-a-session");
        assert!(!chrome.should_render("tab-a-session"));
    }

    #[test]
    fn header_rerenders_on_session_switch() {
        let mut chrome = SessionChromeState::default();
        chrome.mark_rendered("tab-a-session");
        assert!(chrome.should_render("tab-b-session"));
    }
}
