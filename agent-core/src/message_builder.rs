//! Chat message construction utilities for OpenAI/Anthropic API formats.

use serde_json::{json, Value};

use crate::tools::AgentToolCall;
use crate::turn_engine::should_surface_assistant_text;

// ---------------------------------------------------------------------------
// Provider utility functions
// ---------------------------------------------------------------------------

pub fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "minimax" => "MiniMax Chat Completions",
        "deepseek" => "DeepSeek Chat Completions",
        _ => "Chat Completions",
    }
}

pub fn provider_supports_required_tool_choice(provider: &str) -> bool {
    matches!(provider, "minimax")
}

pub fn effective_tool_choice_for_provider<'a>(provider: &str, requested: &'a str) -> (&'a str, bool) {
    if requested == "required" && !provider_supports_required_tool_choice(provider) {
        ("auto", true)
    } else {
        (requested, false)
    }
}

pub fn provider_supports_transport(provider: &str) -> bool {
    matches!(provider, "minimax" | "deepseek")
}

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

pub fn visible_text_message(role: &str, text: &str) -> Value {
    json!({
        "type": role,
        "message": {
            "content": [
                {
                    "type": "text",
                    "text": text,
                }
            ]
        }
    })
}

pub fn visible_assistant_message(text: &str, tool_calls: &[AgentToolCall]) -> Value {
    let mut content = Vec::new();
    if should_surface_assistant_text(text, tool_calls) {
        content.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    for call in tool_calls {
        let parsed_input =
            serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({}));
        content.push(json!({
            "type": "tool_use",
            "id": call.call_id,
            "name": call.tool_name,
            "input": parsed_input,
        }));
    }
    json!({
        "type": "assistant",
        "message": {
            "content": content,
        }
    })
}

pub fn visible_tool_result_message(call_id: &str, preview: &str, is_error: bool) -> Value {
    json!({
        "type": "user",
        "message": {
            "content": [
                {
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": preview,
                    "is_error": is_error,
                }
            ]
        }
    })
}

pub fn hidden_chat_message(message: Value) -> Value {
    json!({
        "type": "chat_message",
        "message": message,
    })
}

// ---------------------------------------------------------------------------
// Extraction functions
// ---------------------------------------------------------------------------

pub fn extract_text_segments(message: &Value) -> Vec<String> {
    message
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let kind = item.get("type").and_then(Value::as_str)?;
                    match kind {
                        "text" => item.get("text").and_then(Value::as_str).map(str::to_string),
                        "tool_result" => item
                            .get("content")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn extract_text_blocks_only(message: &Value) -> Vec<String> {
    message
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) != Some("text") {
                        return None;
                    }
                    item.get("text").and_then(Value::as_str).map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn extract_tool_use_blocks(message: &Value) -> Vec<Value> {
    message
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) != Some("tool_use") {
                        return None;
                    }
                    let input = item.get("input").cloned().unwrap_or_else(|| json!({}));
                    Some(json!({
                        "id": item.get("id").cloned().unwrap_or(Value::Null),
                        "type": "function",
                        "function": {
                            "name": item.get("name").cloned().unwrap_or(Value::Null),
                            "arguments": serde_json::to_string(&input)
                                .unwrap_or_else(|_| "{}".to_string()),
                        }
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn extract_tool_result_blocks(message: &Value) -> Vec<Value> {
    message
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) != Some("tool_result") {
                        return None;
                    }
                    Some(json!({
                        "role": "tool",
                        "tool_call_id": item.get("tool_use_id").cloned().unwrap_or(Value::Null),
                        "content": item.get("content").and_then(Value::as_str).unwrap_or_default(),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Raw assistant message (Chat Completions format)
// ---------------------------------------------------------------------------

/// Build a raw assistant message in Chat Completions wire format.
///
/// `content`, `reasoning_details`, and `tool_calls` are the accumulated
/// fields from a streamed Chat Completions response.
pub fn raw_assistant_message(
    content: &str,
    reasoning_details: &[Value],
    tool_calls: &[Value],
) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": if content.is_empty() {
            Value::Null
        } else {
            Value::String(content.to_string())
        },
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls.to_vec());
    }
    if !reasoning_details.is_empty() {
        message["reasoning_details"] = Value::Array(reasoning_details.to_vec());
    }
    message
}
