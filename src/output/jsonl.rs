use serde_json::json;

use crate::protocol::events::StreamEventEnvelope;

fn to_json_line(envelope: &StreamEventEnvelope) -> String {
    serde_json::to_string(envelope).unwrap_or_else(|_| "{}".to_string())
}

pub fn encode_status(tab_id: &str, stage: &str, message: &str) -> String {
    to_json_line(&StreamEventEnvelope::status(tab_id, stage, message))
}

pub fn encode_complete(tab_id: &str, outcome: &str) -> String {
    json!({
        "tabId": tab_id,
        "payload": {
            "type": "complete",
            "outcome": outcome,
        },
        "protocolVersion": crate::protocol::version(),
    })
    .to_string()
}

pub fn encode_error(tab_id: &str, code: &str, message: &str) -> String {
    to_json_line(&StreamEventEnvelope::error(tab_id, code, message))
}
