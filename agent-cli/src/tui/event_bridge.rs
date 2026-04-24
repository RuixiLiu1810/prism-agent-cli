use agent_core::{
    AgentCompletePayload, AgentEventEnvelope, AgentEventPayload, EventSink,
};

use super::types::ViewUpdate;

#[derive(Debug, Clone)]
pub enum TuiRuntimeEvent {
    AgentEvent(AgentEventEnvelope),
    AgentComplete(AgentCompletePayload),
}

pub struct ChannelEventSink {
    tx: tokio::sync::mpsc::UnboundedSender<TuiRuntimeEvent>,
}

impl ChannelEventSink {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<TuiRuntimeEvent>) -> Self {
        Self { tx }
    }
}

impl EventSink for ChannelEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
        let _ = self.tx.send(TuiRuntimeEvent::AgentEvent(envelope.clone()));
    }

    fn emit_complete(&self, payload: &AgentCompletePayload) {
        let _ = self.tx.send(TuiRuntimeEvent::AgentComplete(payload.clone()));
    }
}

pub fn map_payload(payload: &AgentEventPayload) -> Option<ViewUpdate> {
    match payload {
        AgentEventPayload::MessageDelta(delta) => {
            let text = delta.delta.trim();
            if text.is_empty() {
                None
            } else {
                Some(ViewUpdate::AssistantDelta(text.to_string()))
            }
        }
        AgentEventPayload::ToolResult(result) => {
            if result
                .content
                .get("approvalRequired")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                Some(ViewUpdate::WaitingApproval(
                    "run /approve shell once or /approve shell session".to_string(),
                ))
            } else {
                Some(ViewUpdate::Semantic {
                    text: result.preview.clone(),
                    details: vec![format!(
                        "tool={} call_id={} is_error={}",
                        result.tool_name, result.call_id, result.is_error
                    )],
                })
            }
        }
        AgentEventPayload::Error(err) => Some(ViewUpdate::Error(err.message.clone())),
        AgentEventPayload::Status(status) => {
            if status.stage == "awaiting_approval" {
                Some(ViewUpdate::WaitingApproval(status.message.clone()))
            } else {
                Some(ViewUpdate::Semantic {
                    text: format!("[{}] {}", status.stage, status.message),
                    details: Vec::new(),
                })
            }
        }
        _ => None,
    }
}

pub fn map_complete(payload: &AgentCompletePayload) -> Option<ViewUpdate> {
    if payload.outcome.is_empty() {
        None
    } else {
        Some(ViewUpdate::TurnOutcome(payload.outcome.clone()))
    }
}

#[cfg(test)]
mod tests {
    use agent_core::{AgentErrorEvent, AgentStatusEvent, AgentToolResultEvent};

    use super::*;

    #[test]
    fn maps_tool_result_to_semantic_update() {
        let payload = AgentEventPayload::ToolResult(AgentToolResultEvent {
            tool_name: "read_file".to_string(),
            call_id: "call-1".to_string(),
            is_error: false,
            preview: "Read src/main.rs".to_string(),
            content: serde_json::json!({"path":"src/main.rs"}),
            display: serde_json::Value::Null,
        });

        let update = map_payload(&payload).unwrap_or_else(|| panic!("must map"));
        assert!(matches!(update, ViewUpdate::Semantic { .. }));
    }

    #[test]
    fn maps_approval_required_to_waiting_approval() {
        let payload = AgentEventPayload::ToolResult(AgentToolResultEvent {
            tool_name: "run_shell_command".to_string(),
            call_id: "call-2".to_string(),
            is_error: true,
            preview: "requires approval".to_string(),
            content: serde_json::json!({"approvalRequired": true}),
            display: serde_json::Value::Null,
        });
        let update = map_payload(&payload).unwrap_or_else(|| panic!("must map"));
        assert!(matches!(update, ViewUpdate::WaitingApproval(_)));
    }

    #[test]
    fn maps_error_payload_to_semantic_error_line() {
        let payload = AgentEventPayload::Error(AgentErrorEvent {
            code: "turn_loop_failed".to_string(),
            message: "network down".to_string(),
        });
        let update = map_payload(&payload).unwrap_or_else(|| panic!("must map"));
        assert!(matches!(update, ViewUpdate::Error(_)));
    }

    #[test]
    fn maps_non_waiting_status_to_semantic_update() {
        let payload = AgentEventPayload::Status(AgentStatusEvent {
            stage: "streaming".to_string(),
            message: "connected".to_string(),
        });
        let update = map_payload(&payload).unwrap_or_else(|| panic!("must map"));
        assert!(matches!(update, ViewUpdate::Semantic { .. }));
    }

    #[test]
    fn maps_complete_into_turn_outcome() {
        let complete = AgentCompletePayload {
            tab_id: "tab-1".to_string(),
            outcome: "suspended".to_string(),
        };
        let update = map_complete(&complete).unwrap_or_else(|| panic!("must map"));
        assert_eq!(update, ViewUpdate::TurnOutcome("suspended".to_string()));
    }
}
