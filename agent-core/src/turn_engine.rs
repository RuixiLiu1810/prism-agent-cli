use serde_json::{Value, json};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tokio::sync::watch;

use crate::event_sink::EventSink;
use crate::events::*;
use crate::provider::AgentTurnDescriptor;
use crate::tools::{
    AgentToolCall, AgentToolResult, is_reviewable_edit_tool, tool_result_requires_approval,
    tool_result_review_ready,
};

// ─── Data Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExecutedToolCall {
    pub result: AgentToolResult,
}

#[derive(Debug, Clone)]
pub struct ExecutedToolBatch {
    pub executed: Vec<ExecutedToolCall>,
    pub suspended: bool,
}

// ─── Turn Budget ────────────────────────────────────────────────────

pub const AGENT_CANCELLED_MESSAGE: &str = "Agent run cancelled by user.";

#[derive(Debug, Clone)]
pub struct TurnBudget {
    pub max_rounds: u32,
    pub max_output_tokens: Option<u32>,
    pub consumed_output_tokens: u32,
    pub abort_rx: Option<watch::Receiver<bool>>,
}

fn derive_turn_output_budget(max_rounds: u32, per_call_max_output_tokens: u32) -> u32 {
    let round_multiplier = max_rounds.clamp(1, 4);
    let scaled = per_call_max_output_tokens.saturating_mul(round_multiplier);
    scaled.clamp(8_192, 32_768)
}

impl TurnBudget {
    pub fn new(
        max_rounds: u32,
        max_output_tokens: Option<u32>,
        abort_rx: Option<watch::Receiver<bool>>,
    ) -> Self {
        Self {
            max_rounds,
            max_output_tokens: max_output_tokens
                .map(|per_call| derive_turn_output_budget(max_rounds, per_call)),
            consumed_output_tokens: 0,
            abort_rx,
        }
    }

    pub fn clone_abort_rx(&self) -> Option<watch::Receiver<bool>> {
        self.abort_rx.clone()
    }

    pub fn ensure_round_available(&self, round_index: u32) -> Result<(), String> {
        self.ensure_not_cancelled()?;
        if round_index >= self.max_rounds {
            return Err(format!(
                "Agent turn exceeded the configured round budget of {}.",
                self.max_rounds
            ));
        }
        Ok(())
    }

    pub fn ensure_not_cancelled(&self) -> Result<(), String> {
        if self
            .abort_rx
            .as_ref()
            .map(|rx| *rx.borrow())
            .unwrap_or(false)
        {
            Err(AGENT_CANCELLED_MESSAGE.to_string())
        } else {
            Ok(())
        }
    }

    pub fn record_output_text(&mut self, text: &str) -> Result<(), String> {
        self.consumed_output_tokens = self
            .consumed_output_tokens
            .saturating_add(estimate_tokens(text));
        if let Some(limit) = self.max_output_tokens {
            if self.consumed_output_tokens > limit {
                return Err(format!(
                    "Agent turn exceeded the configured output budget of {} tokens.",
                    limit
                ));
            }
        }
        Ok(())
    }
}

// ─── Tool Call Loop Guard ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolCallTracker {
    call_counts: HashMap<(String, u64), u32>,
    pending_warnings: Vec<String>,
    files_read: Vec<String>,
    files_edited: Vec<String>,
    shells_run: Vec<String>,
    pub current_round: u32,
    pub max_rounds: u32,
}

impl ToolCallTracker {
    pub fn new(max_rounds: u32) -> Self {
        Self {
            call_counts: HashMap::new(),
            pending_warnings: Vec::new(),
            files_read: Vec::new(),
            files_edited: Vec::new(),
            shells_run: Vec::new(),
            current_round: 0,
            max_rounds,
        }
    }

    fn hash_args(args: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        args.trim().hash(&mut hasher);
        hasher.finish()
    }

    pub fn record_call(&mut self, tool_name: &str, args_json: &str) -> u32 {
        let key = (tool_name.to_string(), Self::hash_args(args_json));
        let count = self.call_counts.entry(key).or_insert(0);
        *count += 1;
        let current = *count;

        if current >= 3 {
            self.pending_warnings.push(format!(
                "[Loop detected] You have called {} with the same arguments {} times. \
                 STOP calling this tool. Use previous results or take a different approach. \
                 If your edit is complete, summarize the changes to the user.",
                tool_name, current
            ));
        } else if current >= 2 {
            self.pending_warnings.push(format!(
                "[Repetition notice] You already called {} with similar arguments. \
                 Use the previous result instead of re-calling.",
                tool_name
            ));
        }

        if let Ok(args) = serde_json::from_str::<Value>(args_json) {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .or_else(|| args.get("file_path").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            match tool_name {
                "read_file" => {
                    if !path.is_empty() && !self.files_read.contains(&path) {
                        self.files_read.push(path);
                    }
                }
                "apply_text_patch" | "replace_selected_text" | "write_file" => {
                    if !path.is_empty() && !self.files_edited.contains(&path) {
                        self.files_edited.push(path);
                    }
                }
                "run_shell_command" => {
                    if let Some(cmd) = args.get("command").and_then(Value::as_str) {
                        let short = if cmd.len() > 60 { &cmd[..60] } else { cmd };
                        self.shells_run.push(short.to_string());
                    }
                }
                _ => {}
            }
        }

        current
    }

    pub fn progress_checkpoint(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "[Progress checkpoint — round {}/{}]",
            self.current_round + 1,
            self.max_rounds
        ));
        if !self.files_read.is_empty() {
            parts.push(format!("- Files read: {}", self.files_read.join(", ")));
        }
        if !self.files_edited.is_empty() {
            parts.push(format!("- Files edited: {}", self.files_edited.join(", ")));
        }
        if !self.shells_run.is_empty() {
            let display: Vec<&str> = self.shells_run.iter().map(|s| s.as_str()).take(5).collect();
            parts.push(format!("- Commands run: {}", display.join("; ")));
        }
        parts.push(format!(
            "- Remaining budget: {} rounds",
            self.max_rounds.saturating_sub(self.current_round + 1)
        ));
        parts.push(
            "If your task is complete, respond to the user now. \
             Do not verify successful edits with shell commands."
                .to_string(),
        );
        parts.join("\n")
    }

    pub fn build_injection(&mut self, round_idx: u32) -> Option<String> {
        let has_warnings = !self.pending_warnings.is_empty();
        let should_checkpoint = (round_idx + 1) % 4 == 0 || has_warnings;
        if !should_checkpoint {
            self.pending_warnings.clear();
            return None;
        }

        let mut msg = self.progress_checkpoint();
        for w in self.pending_warnings.drain(..) {
            msg.push('\n');
            msg.push_str(&w);
        }
        Some(msg)
    }
}

// ─── Token Estimation ───────────────────────────────────────────────

pub fn estimate_tokens(text: &str) -> u32 {
    let mut cjk_chars = 0u32;
    let mut other_chars = 0u32;
    for c in text.chars() {
        if is_cjk_char(c) {
            cjk_chars += 1;
        } else {
            other_chars += 1;
        }
    }
    (cjk_chars * 3 + other_chars).div_ceil(4)
}

fn is_cjk_char(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
}

// ─── History Compaction ─────────────────────────────────────────────

const HISTORY_COMPACT_TOKEN_LIMIT: u32 = 60_000;

fn estimate_message_tokens(msg: &Value) -> u32 {
    let overhead = 4u32;
    let content_tokens = msg
        .get("content")
        .and_then(Value::as_str)
        .map(|s| estimate_tokens(s))
        .unwrap_or(0);
    let tool_calls_tokens = msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|call| {
                    let name_tokens = call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .map(|s| estimate_tokens(s))
                        .unwrap_or(0);
                    let args_tokens = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .map(|s| estimate_tokens(s))
                        .unwrap_or(0);
                    name_tokens + args_tokens + 4
                })
                .sum::<u32>()
        })
        .unwrap_or(0);
    overhead + content_tokens + tool_calls_tokens
}

pub fn estimate_messages_tokens(messages: &[Value]) -> u32 {
    messages.iter().map(|m| estimate_message_tokens(m)).sum()
}

pub fn compact_chat_messages(messages: &mut Vec<Value>) {
    let total_tokens = estimate_messages_tokens(messages);
    if total_tokens <= HISTORY_COMPACT_TOKEN_LIMIT || messages.len() <= 3 {
        return;
    }

    let mut segment_starts: Vec<usize> = vec![1];
    for i in 2..messages.len() {
        let role = messages[i]
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("");
        if role != "tool" {
            segment_starts.push(i);
        }
    }

    let system_tokens = estimate_message_tokens(&messages[0]);
    let summary_reserve = 200u32;
    let available = HISTORY_COMPACT_TOKEN_LIMIT
        .saturating_sub(system_tokens)
        .saturating_sub(summary_reserve);

    let mut tail_tokens = 0u32;
    let mut keep_from_seg = segment_starts.len();
    for seg_idx in (0..segment_starts.len()).rev() {
        let seg_start = segment_starts[seg_idx];
        let seg_end = if seg_idx + 1 < segment_starts.len() {
            segment_starts[seg_idx + 1]
        } else {
            messages.len()
        };
        let seg_tokens: u32 = messages[seg_start..seg_end]
            .iter()
            .map(|m| estimate_message_tokens(m))
            .sum();
        if tail_tokens + seg_tokens > available {
            break;
        }
        tail_tokens += seg_tokens;
        keep_from_seg = seg_idx;
    }

    if keep_from_seg == 0 {
        return;
    }
    let cut_point = segment_starts[keep_from_seg];
    if cut_point <= 1 {
        return;
    }

    let dropped = &messages[1..cut_point];
    let dropped_count = dropped.len();
    let mut unique_tools: Vec<&str> = Vec::new();
    for msg in dropped {
        if let Some(calls) = msg.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                if let Some(name) = call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                {
                    if !unique_tools.contains(&name) {
                        unique_tools.push(name);
                    }
                }
            }
        }
    }

    let summary = if unique_tools.is_empty() {
        format!(
            "[Context compacted: {} earlier messages removed to fit context window. \
             Recent conversation preserved below.]",
            dropped_count
        )
    } else {
        format!(
            "[Context compacted: {} earlier messages removed. \
             Tools previously used: {}. Recent context preserved below.]",
            dropped_count,
            unique_tools.join(", ")
        )
    };

    messages.splice(
        1..cut_point,
        std::iter::once(json!({
            "role": "system",
            "content": summary,
        })),
    );
}

// ─── Tool Result Feedback ───────────────────────────────────────────

pub fn request_has_binary_attachment_context(request: &AgentTurnDescriptor) -> bool {
    request.prompt.lines().any(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("[Resource path: ") || !trimmed.ends_with(']') {
            return false;
        }
        let path = trimmed
            .trim_start_matches("[Resource path: ")
            .trim_end_matches(']')
            .trim()
            .to_ascii_lowercase();
        path.ends_with(".pdf") || path.ends_with(".docx")
    })
}

pub fn should_surface_assistant_text(text: &str, tool_calls: &[AgentToolCall]) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    !tool_calls
        .iter()
        .any(|call| is_reviewable_edit_tool(&call.tool_name))
}

pub fn tool_result_feedback_for_model(result: &AgentToolResult) -> String {
    let raw = tool_result_feedback_for_model_inner(result);
    truncate_tool_feedback(raw, &result.tool_name)
}

const TOOL_RESULT_MAX_CHARS: usize = 4000;

fn truncate_tool_feedback(text: String, tool_name: &str) -> String {
    if text.chars().count() <= TOOL_RESULT_MAX_CHARS {
        return text;
    }
    let truncated: String = text.chars().take(TOOL_RESULT_MAX_CHARS).collect();
    let recovery_hint = match tool_name {
        "read_file" => " Call read_file with a specific line range to see the rest.",
        "run_shell_command" => " The full output was truncated.",
        "read_document"
        | "read_document_excerpt"
        | "search_document_text"
        | "get_document_evidence" => {
            " Use search_document_text with a more specific query to find relevant sections."
        }
        _ => "",
    };
    format!(
        "{}...\n[Output truncated at {} chars.{}]",
        truncated, TOOL_RESULT_MAX_CHARS, recovery_hint
    )
}

fn tool_result_feedback_for_model_inner(result: &AgentToolResult) -> String {
    let approval_required = result
        .content
        .get("approvalRequired")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if result.is_error {
        if approval_required {
            return "The requested edit has been staged for user review and approval. Do not retry this edit unless the user requests a different change.".to_string();
        }

        let error = result
            .content
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Tool execution failed.");

        let correction = if error.contains("not found verbatim")
            || error.contains("expected text was not found")
        {
            "Read the file first and retry with an exact verbatim text match, including whitespace and line breaks."
        } else if error.contains("matched multiple")
            || error.contains("more specific edit tool call")
        {
            "Retry with a longer, more specific exact excerpt that uniquely identifies the target location."
        } else if error.contains("selection-scoped edits must not use write_file") {
            "Use replace_selected_text when a valid selection anchor exists, or read_file followed by apply_text_patch for an exact in-file patch."
        } else if error
            .contains("attachment-backed PDF/DOCX analysis must not use run_shell_command")
            || error.contains(
                "attachment-backed PDF/DOCX analysis must not use read_file on binary resources",
            )
        {
            "Use read_document instead of probing the binary attachment again."
        } else if error.contains("Invalid tool arguments JSON") {
            "Retry with valid JSON arguments and ensure required fields are present."
        } else {
            "Verify the target file and exact input text before retrying."
        };

        return format!("Error: {} {}", error, correction);
    }

    match result.tool_name.as_str() {
        "read_file" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("file");
            let content = result
                .content
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("");
            let truncated = result
                .content
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if truncated {
                format!(
                    "Read {} successfully. File content (truncated):\n{}",
                    path, content
                )
            } else {
                format!("Read {} successfully. File content:\n{}", path, content)
            }
        }
        "apply_text_patch" | "replace_selected_text" | "write_file" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("file");
            let written = result
                .content
                .get("written")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if written {
                format!(
                    "Edit applied successfully to {}. Do not verify this edit with shell commands or re-read the file. Summarize the change to the user.",
                    path
                )
            } else {
                format!("Reviewable edit prepared for {}.", path)
            }
        }
        "inspect_resource" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("resource");
            let kind = result
                .content
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("resource");
            let status = result
                .content
                .get("extractionStatus")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let fallback_used = result
                .content
                .get("fallbackUsed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            format!(
                "Resource inspection for {}: kind={}, extraction_status={}{}.",
                path,
                kind,
                status,
                if fallback_used {
                    ", internal shell fallback available/used"
                } else {
                    ""
                }
            )
        }
        "read_document_excerpt" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("document");
            let excerpt = result
                .content
                .get("excerpt")
                .and_then(Value::as_str)
                .unwrap_or("");
            let fallback_used = result
                .content
                .get("fallbackUsed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if fallback_used {
                format!(
                    "Document excerpt from {} (using internal controlled fallback extraction):\n{}",
                    path, excerpt
                )
            } else {
                format!("Document excerpt from {}:\n{}", path, excerpt)
            }
        }
        "read_document" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("document");
            let excerpt = result
                .content
                .get("excerpt")
                .and_then(Value::as_str)
                .unwrap_or("");
            let query = result.content.get("query").and_then(Value::as_str);
            let fallback_used = result
                .content
                .get("fallbackUsed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let matches = result
                .content
                .get("matches")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .take(4)
                        .filter_map(|entry| {
                            let label = entry.get("label").and_then(Value::as_str)?;
                            let snippet = entry.get("snippet").and_then(Value::as_str)?;
                            Some(format!("- {}: {}", label, snippet))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if let Some(text) = query {
                if matches.is_empty() {
                    format!(
                        "Read document {} but found no relevant evidence for '{}'{}.",
                        path,
                        text,
                        if fallback_used {
                            " after internal fallback extraction"
                        } else {
                            ""
                        }
                    )
                } else {
                    format!(
                        "Relevant document evidence from {} for query '{}'{}:\n{}",
                        path,
                        text,
                        if fallback_used {
                            " (using internal controlled fallback extraction)"
                        } else {
                            ""
                        },
                        matches.join("\n")
                    )
                }
            } else if fallback_used {
                format!(
                    "Document excerpt from {} (using internal controlled fallback extraction):\n{}",
                    path, excerpt
                )
            } else {
                format!("Document excerpt from {}:\n{}", path, excerpt)
            }
        }
        "search_document_text" | "get_document_evidence" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("document");
            let query = result
                .content
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("query");
            let matches = result
                .content
                .get("matches")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .take(4)
                        .filter_map(|entry| {
                            let label = entry.get("label").and_then(Value::as_str)?;
                            let snippet = entry.get("snippet").and_then(Value::as_str)?;
                            Some(format!("- {}: {}", label, snippet))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let fallback_used = result
                .content
                .get("fallbackUsed")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if matches.is_empty() {
                format!(
                    "No relevant document matches were found in {} for query '{}'{}.",
                    path,
                    query,
                    if fallback_used {
                        " after internal fallback extraction"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "Relevant document evidence from {} for query '{}'{}:\n{}",
                    path,
                    query,
                    if fallback_used {
                        " (using internal controlled fallback extraction)"
                    } else {
                        ""
                    },
                    matches.join("\n")
                )
            }
        }
        "draft_section" => {
            let section = result
                .content
                .get("sectionType")
                .and_then(Value::as_str)
                .unwrap_or("section");
            let draft = result
                .content
                .get("draft")
                .and_then(Value::as_str)
                .unwrap_or("");
            format!("Drafted {} content:\n{}", section, draft)
        }
        "restructure_outline" => {
            let count = result
                .content
                .get("revisedOutline")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0);
            let added = result
                .content
                .get("addedSectionCount")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            format!(
                "Restructured manuscript outline into {} sections ({} added).",
                count, added
            )
        }
        "check_consistency" => {
            let summary = result
                .content
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("Consistency scan completed.");
            let findings = result
                .content
                .get("findings")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .take(3)
                        .filter_map(|entry| entry.get("message").and_then(Value::as_str))
                        .map(|message| format!("- {}", message))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if findings.is_empty() {
                summary.to_string()
            } else {
                format!("{}\n{}", summary, findings.join("\n"))
            }
        }
        "generate_abstract" => {
            let abstract_text = result
                .content
                .get("abstract")
                .and_then(Value::as_str)
                .unwrap_or("");
            format!("Generated abstract:\n{}", abstract_text)
        }
        "insert_citation" => result
            .content
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Citation insertion completed.".to_string()),
        "search_literature" => {
            let query = result
                .content
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("query");
            let count = result
                .content
                .get("resultCount")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            format!(
                "Literature search for '{}' returned {} candidate papers.",
                query, count
            )
        }
        "analyze_paper" => {
            let path = result
                .content
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("paper");
            let objective = result
                .content
                .get("objective")
                .and_then(Value::as_str)
                .unwrap_or("Objective not available.");
            format!(
                "Paper analysis completed for {}.\nObjective: {}",
                path, objective
            )
        }
        "compare_papers" | "synthesize_evidence" | "extract_methodology" => result.preview.clone(),
        "review_manuscript" => result
            .content
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Peer review scan completed.".to_string()),
        "check_statistics" => result
            .content
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Statistics review completed.".to_string()),
        "verify_references" => result
            .content
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Reference verification completed.".to_string()),
        "generate_response_letter" => {
            let letter = result
                .content
                .get("letter")
                .and_then(Value::as_str)
                .unwrap_or("");
            if letter.is_empty() {
                "Response letter draft generated.".to_string()
            } else {
                format!("Response letter draft:\n{}", letter)
            }
        }
        "track_revisions" => result
            .content
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Revision tracking completed.".to_string()),
        _ => {
            if result.preview.trim().is_empty() {
                "Tool completed successfully.".to_string()
            } else {
                result.preview.clone()
            }
        }
    }
}

pub fn tool_result_has_invalid_arguments_error(result: &AgentToolResult) -> bool {
    if !result.is_error {
        return false;
    }
    result
        .content
        .get("error")
        .and_then(Value::as_str)
        .map(|message| message.contains("Invalid tool arguments JSON"))
        .unwrap_or(false)
}

pub fn tool_result_status(tool_name: &str, result_content: &Value) -> (&'static str, String) {
    let synthetic = AgentToolResult {
        tool_name: tool_name.to_string(),
        call_id: String::new(),
        is_error: false,
        content: result_content.clone(),
        preview: String::new(),
    };
    let approval_required = tool_result_requires_approval(&synthetic);
    let review_ready = tool_result_review_ready(&synthetic);

    if approval_required && review_ready && is_reviewable_edit_tool(tool_name) {
        return (
            "review_ready",
            "Diff is ready for review before the edit is applied.".to_string(),
        );
    }

    if approval_required {
        return (
            "awaiting_approval",
            format!("{} is waiting for approval.", tool_name),
        );
    }

    (
        "tool_result_ready",
        format!("{} finished. Continuing the task...", tool_name),
    )
}

// ─── Event Emission (via EventSink, replaces WebviewWindow) ─────────

pub fn emit_status(sink: &dyn EventSink, tab_id: &str, stage: &str, message: &str) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::Status(AgentStatusEvent {
            stage: stage.to_string(),
            message: message.to_string(),
        }),
    });
}

pub fn emit_error(sink: &dyn EventSink, tab_id: &str, code: &str, message: String) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::Error(AgentErrorEvent {
            code: code.to_string(),
            message,
        }),
    });
}

pub fn emit_text_delta(sink: &dyn EventSink, tab_id: &str, delta: &str) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::MessageDelta(AgentMessageDeltaEvent {
            delta: delta.to_string(),
        }),
    });
}

pub fn emit_tool_call(
    sink: &dyn EventSink,
    tab_id: &str,
    tool_name: &str,
    call_id: &str,
    input: Value,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ToolCall(AgentToolCallEvent {
            tool_name: tool_name.to_string(),
            call_id: call_id.to_string(),
            input,
        }),
    });
}

pub fn emit_tool_result(
    sink: &dyn EventSink,
    tab_id: &str,
    tool_name: &str,
    call_id: &str,
    is_error: bool,
    preview: String,
    content: Value,
    display: Value,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ToolResult(AgentToolResultEvent {
            tool_name: tool_name.to_string(),
            call_id: call_id.to_string(),
            is_error,
            preview,
            content,
            display,
        }),
    });
}

pub fn emit_tool_resumed(
    sink: &dyn EventSink,
    tab_id: &str,
    tool_name: &str,
    target_path: Option<&str>,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ToolResumed(AgentToolResumedEvent {
            tool_name: tool_name.to_string(),
            target_path: target_path.map(str::to_string),
            message: message.to_string(),
        }),
    });
    emit_tool_interrupt_state(
        sink,
        tab_id,
        AgentToolInterruptPhase::Resumed,
        Some(tool_name),
        None,
        target_path,
        Some(tool_name),
        false,
        false,
        message,
    );
}

pub fn emit_turn_resumed(
    sink: &dyn EventSink,
    tab_id: &str,
    local_session_id: Option<&str>,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::TurnResumed(AgentTurnResumedEvent {
            local_session_id: local_session_id.map(str::to_string),
            message: message.to_string(),
        }),
    });
    emit_tool_interrupt_state(
        sink,
        tab_id,
        AgentToolInterruptPhase::Cleared,
        None,
        None,
        None,
        None,
        false,
        false,
        message,
    );
}

pub fn emit_workflow_checkpoint_requested(
    sink: &dyn EventSink,
    tab_id: &str,
    workflow_type: &str,
    stage: &str,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::WorkflowCheckpointRequested(
            AgentWorkflowCheckpointRequestedEvent {
                workflow_type: workflow_type.to_string(),
                stage: stage.to_string(),
                message: message.to_string(),
            },
        ),
    });
}

pub fn emit_workflow_checkpoint_approved(
    sink: &dyn EventSink,
    tab_id: &str,
    workflow_type: &str,
    from_stage: &str,
    to_stage: &str,
    completed: bool,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::WorkflowCheckpointApproved(
            AgentWorkflowCheckpointApprovedEvent {
                workflow_type: workflow_type.to_string(),
                from_stage: from_stage.to_string(),
                to_stage: to_stage.to_string(),
                completed,
                message: message.to_string(),
            },
        ),
    });
}

pub fn emit_workflow_checkpoint_rejected(
    sink: &dyn EventSink,
    tab_id: &str,
    workflow_type: &str,
    stage: &str,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::WorkflowCheckpointRejected(
            AgentWorkflowCheckpointRejectedEvent {
                workflow_type: workflow_type.to_string(),
                stage: stage.to_string(),
                message: message.to_string(),
            },
        ),
    });
}

pub fn emit_tool_interrupt_state(
    sink: &dyn EventSink,
    tab_id: &str,
    phase: AgentToolInterruptPhase,
    tool_name: Option<&str>,
    call_id: Option<&str>,
    target_path: Option<&str>,
    approval_tool_name: Option<&str>,
    review_ready: bool,
    can_resume: bool,
    message: &str,
) {
    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ToolInterrupt(AgentToolInterruptEvent {
            phase,
            tool_name: tool_name.map(str::to_string),
            call_id: call_id.map(str::to_string),
            target_path: target_path.map(str::to_string),
            approval_tool_name: approval_tool_name.map(str::to_string),
            review_ready,
            can_resume,
            message: message.to_string(),
        }),
    });
}

pub fn emit_approval_requested(
    sink: &dyn EventSink,
    tab_id: &str,
    tool_name: &str,
    call_id: &str,
    content: &Value,
) {
    let approval_required = content
        .get("approvalRequired")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !approval_required {
        return;
    }

    let target_path = content
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string);
    let review_ready = content
        .get("reviewArtifact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let message = content
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("Tool approval is required.")
        .to_string();

    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ApprovalRequested(AgentApprovalRequestedEvent {
            tool_name: tool_name.to_string(),
            call_id: call_id.to_string(),
            target_path,
            review_ready,
            message,
        }),
    });
}

pub fn emit_review_artifact_ready(
    sink: &dyn EventSink,
    tab_id: &str,
    tool_name: &str,
    call_id: &str,
    content: &Value,
) {
    let Some(path) = content.get("path").and_then(Value::as_str) else {
        return;
    };
    let review_ready = content
        .get("reviewArtifact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !review_ready {
        return;
    }

    let summary = content
        .get("reviewArtifactPayload")
        .and_then(Value::as_object)
        .and_then(|payload| payload.get("summary"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            content
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let written = content
        .get("written")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    sink.emit_event(&AgentEventEnvelope {
        tab_id: tab_id.to_string(),
        payload: AgentEventPayload::ReviewArtifactReady(AgentReviewArtifactReadyEvent {
            tool_name: tool_name.to_string(),
            call_id: call_id.to_string(),
            target_path: path.to_string(),
            summary,
            written,
        }),
    });
}

pub fn emit_agent_complete(sink: &dyn EventSink, tab_id: &str, outcome: &str) {
    sink.emit_complete(&AgentCompletePayload {
        tab_id: tab_id.to_string(),
        outcome: outcome.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_sink::test_util::VecEventSink;

    #[test]
    fn estimate_tokens_ascii() {
        // "hello world" = 11 chars, ~3 tokens
        let t = estimate_tokens("hello world");
        assert!(t >= 2 && t <= 4);
    }

    #[test]
    fn estimate_tokens_cjk() {
        // 4 CJK chars → 4*3/4 = 3 tokens
        let t = estimate_tokens("你好世界");
        assert_eq!(t, 3);
    }

    #[test]
    fn turn_budget_cancel() {
        let (tx, rx) = watch::channel(false);
        let budget = TurnBudget::new(10, None, Some(rx));
        assert!(budget.ensure_not_cancelled().is_ok());
        let _ = tx.send(true);
        assert!(budget.ensure_not_cancelled().is_err());
    }

    #[test]
    fn turn_budget_round_limit() {
        let budget = TurnBudget::new(3, None, None);
        assert!(budget.ensure_round_available(0).is_ok());
        assert!(budget.ensure_round_available(2).is_ok());
        assert!(budget.ensure_round_available(3).is_err());
    }

    #[test]
    fn tool_call_tracker_detects_loops() {
        let mut tracker = ToolCallTracker::new(10);
        assert_eq!(tracker.record_call("read_file", r#"{"path":"a.txt"}"#), 1);
        assert_eq!(tracker.record_call("read_file", r#"{"path":"a.txt"}"#), 2);
        assert_eq!(tracker.record_call("read_file", r#"{"path":"a.txt"}"#), 3);
        let injection = tracker.build_injection(0);
        assert!(injection.is_some());
        let msg = injection.unwrap();
        assert!(msg.contains("Loop detected"));
    }

    #[test]
    fn compact_messages_noop_when_small() {
        let mut messages = vec![
            json!({"role": "system", "content": "You are an assistant."}),
            json!({"role": "user", "content": "Hello"}),
            json!({"role": "assistant", "content": "Hi!"}),
        ];
        let original_len = messages.len();
        compact_chat_messages(&mut messages);
        assert_eq!(messages.len(), original_len);
    }

    #[test]
    fn emit_status_uses_sink() {
        let sink = VecEventSink::new();
        emit_status(&sink, "tab1", "init", "Starting...");
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tab_id, "tab1");
        match &events[0].payload {
            AgentEventPayload::Status(s) => {
                assert_eq!(s.stage, "init");
                assert_eq!(s.message, "Starting...");
            }
            _ => panic!("Expected Status event"),
        }
    }

    #[test]
    fn emit_agent_complete_uses_sink() {
        let sink = VecEventSink::new();
        emit_agent_complete(&sink, "tab1", "success");
        let completes = sink.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0].outcome, "success");
    }

    #[test]
    fn should_surface_text_hides_edit_tool_text() {
        let calls = vec![AgentToolCall {
            tool_name: "apply_text_patch".to_string(),
            call_id: "c1".to_string(),
            arguments: "{}".to_string(),
        }];
        assert!(!should_surface_assistant_text("Some text", &calls));
    }

    #[test]
    fn should_surface_text_shows_non_edit_text() {
        let calls = vec![AgentToolCall {
            tool_name: "read_file".to_string(),
            call_id: "c1".to_string(),
            arguments: "{}".to_string(),
        }];
        assert!(should_surface_assistant_text("Some text", &calls));
    }
}
