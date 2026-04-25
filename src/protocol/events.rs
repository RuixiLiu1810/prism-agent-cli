use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamEventEnvelope {
    pub tab_id: String,
    pub payload: StreamEventPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventPayload {
    Status(StreamStatusPayload),
    Complete(StreamCompletePayload),
    Error(StreamErrorPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamStatusPayload {
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamCompletePayload {
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamErrorPayload {
    pub code: String,
    pub message: String,
}

impl StreamEventEnvelope {
    pub fn status(tab_id: &str, stage: &str, message: &str) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            payload: StreamEventPayload::Status(StreamStatusPayload {
                stage: stage.to_string(),
                message: message.to_string(),
            }),
            protocol_version: None,
        }
    }

    pub fn complete(tab_id: &str, outcome: &str, protocol_version: u32) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            payload: StreamEventPayload::Complete(StreamCompletePayload {
                outcome: outcome.to_string(),
            }),
            protocol_version: Some(protocol_version),
        }
    }

    pub fn error(tab_id: &str, code: &str, message: &str) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            payload: StreamEventPayload::Error(StreamErrorPayload {
                code: code.to_string(),
                message: message.to_string(),
            }),
            protocol_version: None,
        }
    }
}
