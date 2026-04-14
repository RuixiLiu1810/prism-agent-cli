//! SSE streaming parser and shared HTTP response utilities.

use serde_json::{Value, json};

use crate::config::AgentSamplingProfilesConfig;
use crate::provider::AgentSamplingProfile;
use crate::tools::AgentToolCall;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TOOL_ARGUMENTS_RETRY_HINT: &str = "[Tool argument recovery rule]\n\
The previous tool call arguments were invalid JSON. Retry by emitting tool arguments as a strict JSON object only (no markdown fences, no prose, no trailing commentary). Include every required field.";

// ---------------------------------------------------------------------------
// SSE frame parsing
// ---------------------------------------------------------------------------

pub fn parse_sse_frame(frame: &str) -> Option<(String, String)> {
    let mut event_name: Option<String> = None;
    let mut data_lines = Vec::new();

    for raw_line in frame.lines() {
        let line = raw_line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some((
        event_name.unwrap_or_else(|| "message".to_string()),
        data_lines.join("\n"),
    ))
}

pub fn take_next_sse_frame(buffer: &mut String) -> Option<(String, String)> {
    let ends = [
        buffer.find("\r\n\r\n").map(|idx| (idx, 4)),
        buffer.find("\n\n").map(|idx| (idx, 2)),
    ]
    .into_iter()
    .flatten()
    .min_by_key(|(idx, _)| *idx);

    let (idx, sep_len) = ends?;
    let frame = buffer[..idx].to_string();
    buffer.drain(..idx + sep_len);
    parse_sse_frame(&frame)
}

// ---------------------------------------------------------------------------
// Response extraction helpers
// ---------------------------------------------------------------------------

pub fn extract_response_id(payload: &Value) -> Option<String> {
    payload
        .pointer("/response/id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("response_id").and_then(Value::as_str))
        .or_else(|| payload.get("id").and_then(Value::as_str))
        .map(str::to_string)
}

pub fn extract_function_call_item(item: &Value) -> Option<AgentToolCall> {
    if item.get("type").and_then(Value::as_str) != Some("function_call") {
        return None;
    }

    let tool_name = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)?;
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| item.get("id").and_then(Value::as_str))
        .map(str::to_string)?;
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}")
        .to_string();

    Some(AgentToolCall {
        tool_name,
        call_id,
        arguments,
    })
}

// ---------------------------------------------------------------------------
// Sampling profile resolution
// ---------------------------------------------------------------------------

pub fn sampling_profile_params(
    profile: Option<&AgentSamplingProfile>,
    config: Option<&AgentSamplingProfilesConfig>,
) -> Option<(f64, f64, u32)> {
    match profile {
        Some(AgentSamplingProfile::EditStable) => config
            .map(|profiles| {
                (
                    profiles.edit_stable.temperature,
                    profiles.edit_stable.top_p,
                    profiles.edit_stable.max_tokens,
                )
            })
            .or(Some((0.2, 0.9, 8192))),
        Some(AgentSamplingProfile::AnalysisBalanced) => config
            .map(|profiles| {
                (
                    profiles.analysis_balanced.temperature,
                    profiles.analysis_balanced.top_p,
                    profiles.analysis_balanced.max_tokens,
                )
            })
            .or(Some((0.4, 0.9, 6144))),
        Some(AgentSamplingProfile::AnalysisDeep) => config
            .map(|profiles| {
                (
                    profiles.analysis_deep.temperature,
                    profiles.analysis_deep.top_p,
                    profiles.analysis_deep.max_tokens,
                )
            })
            .or(Some((0.3, 0.92, 12288))),
        Some(AgentSamplingProfile::ChatFlexible) => config
            .map(|profiles| {
                (
                    profiles.chat_flexible.temperature,
                    profiles.chat_flexible.top_p,
                    profiles.chat_flexible.max_tokens,
                )
            })
            .or(Some((0.7, 0.95, 4096))),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Stream fragment merging
// ---------------------------------------------------------------------------

pub fn merge_stream_fragment(existing: &str, incoming: &str) -> String {
    if incoming.is_empty() {
        return String::new();
    }
    if existing.is_empty() {
        return incoming.to_string();
    }
    if incoming.starts_with(existing) {
        return incoming[existing.len()..].to_string();
    }
    incoming.to_string()
}

pub fn push_reasoning_delta(reasoning_details: &mut Vec<Value>, delta: &Value) {
    let Some(items) = delta.get("reasoning_details").and_then(Value::as_array) else {
        return;
    };

    while reasoning_details.len() < items.len() {
        reasoning_details.push(json!({}));
    }

    for (index, item) in items.iter().enumerate() {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        let target = reasoning_details[index]
            .as_object_mut()
            .expect("reasoning detail should stay object");

        for (key, value) in item_obj {
            if key == "text" {
                let existing = target
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let incoming = value.as_str().unwrap_or_default();
                let merged = if existing.is_empty() {
                    incoming.to_string()
                } else {
                    let delta = merge_stream_fragment(existing, incoming);
                    format!("{}{}", existing, delta)
                };
                target.insert("text".to_string(), Value::String(merged));
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}
