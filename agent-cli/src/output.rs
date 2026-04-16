use std::io::{self, Write};
use std::sync::Mutex;

use agent_core::{
    AgentCompletePayload, AgentErrorEvent, AgentEventEnvelope, AgentEventPayload,
    AgentToolCallEvent, AgentToolResultEvent, EventSink,
};

fn write_line<W: Write>(writer: &mut W, line: &str) {
    let _ = writer.write_all(line.as_bytes());
    let _ = writer.flush();
}

pub struct JsonlEventSink {
    writer: Mutex<Vec<u8>>,
    mirror_stdout: bool,
}

impl JsonlEventSink {
    pub fn stdout() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: true,
        }
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: false,
        }
    }

    #[cfg(test)]
    pub fn take_test_output(&self) -> String {
        let mut guard = match self.writer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let out = String::from_utf8_lossy(&guard).to_string();
        guard.clear();
        out
    }

    fn emit_json<T: serde::Serialize>(&self, value: &T) {
        if let Ok(json) = serde_json::to_string(value) {
            if let Ok(mut guard) = self.writer.lock() {
                write_line(&mut *guard, &(json.clone() + "\n"));
            }
            if self.mirror_stdout {
                let stdout = io::stdout();
                let mut handle = stdout.lock();
                write_line(&mut handle, &(json + "\n"));
            }
        }
    }
}

impl EventSink for JsonlEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
        self.emit_json(envelope);
    }

    fn emit_complete(&self, payload: &AgentCompletePayload) {
        self.emit_json(payload);
    }
}

pub struct HumanEventSink {
    writer: Mutex<Vec<u8>>,
    mirror_stdout: bool,
}

impl HumanEventSink {
    pub fn stdout() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: true,
        }
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            writer: Mutex::new(Vec::new()),
            mirror_stdout: false,
        }
    }

    #[cfg(test)]
    pub fn take_test_output(&self) -> String {
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

    fn render_tool_call(call: &AgentToolCallEvent) -> String {
        format!("\n[tool] {} ({})\n", call.tool_name, call.call_id)
    }

    fn render_tool_result(result: &AgentToolResultEvent) -> String {
        format!("\n[result] {}\n", result.preview)
    }

    fn render_error(error: &AgentErrorEvent) -> String {
        format!("\n[error:{}] {}\n", error.code, error.message)
    }
}

impl EventSink for HumanEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
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
        self.write_human(&format!("\n[turn:{}]\n", payload.outcome));
    }
}

#[cfg(test)]
mod tests {
    use super::{HumanEventSink, JsonlEventSink};
    use agent_core::{
        AgentCompletePayload, AgentEventEnvelope, AgentEventPayload, AgentMessageDeltaEvent,
        AgentStatusEvent, EventSink,
    };

    #[test]
    fn human_sink_formats_status_and_delta() {
        let sink = HumanEventSink::for_test();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "thinking".to_string(),
                message: "Planning".to_string(),
            }),
        });
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::MessageDelta(AgentMessageDeltaEvent {
                delta: "hello".to_string(),
            }),
        });

        let out = sink.take_test_output();
        assert!(out.contains("[thinking] Planning"));
        assert!(out.contains("hello"));
        assert!(!out.contains("\"payload\""));
    }

    #[test]
    fn jsonl_sink_writes_serialized_complete_payload() {
        let sink = JsonlEventSink::for_test();
        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "completed".to_string(),
        });
        let out = sink.take_test_output();
        assert!(out.contains("\"outcome\":\"completed\""));
    }
}
