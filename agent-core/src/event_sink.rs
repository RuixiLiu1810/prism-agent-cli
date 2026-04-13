use crate::events::{AgentCompletePayload, AgentEventEnvelope, AgentEventPayload};

/// Abstraction over event emission — replaces direct `WebviewWindow::emit` calls.
///
/// Tauri adapter implements this by forwarding to `window.emit(...)`.
/// CLI implements this by writing JSON Lines to stdout.
/// Tests use `NullEventSink` or a collecting `VecEventSink`.
pub trait EventSink: Send + Sync {
    /// Emit an agent event (status, delta, tool_call, tool_result, etc.).
    fn emit_event(&self, envelope: &AgentEventEnvelope);

    /// Emit the agent-complete signal (separate Tauri event channel).
    fn emit_complete(&self, payload: &AgentCompletePayload);
}

// ── Convenience constructors ────────────────────────────────────────

impl dyn EventSink {
    /// Build an `AgentEventEnvelope` and emit it.
    pub fn emit(&self, tab_id: &str, payload: AgentEventPayload) {
        self.emit_event(&AgentEventEnvelope {
            tab_id: tab_id.to_string(),
            payload,
        });
    }
}

// ── Null implementation (silent, for tests / headless runs) ─────────

/// A no-op event sink that discards all events.
pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit_event(&self, _envelope: &AgentEventEnvelope) {}
    fn emit_complete(&self, _payload: &AgentCompletePayload) {}
}

// ── Vec-collecting implementation (for tests) ───────────────────────

#[cfg(any(test, feature = "test-util"))]
pub mod test_util {
    use super::*;
    use std::sync::Mutex;

    /// Collects all emitted events into a `Vec` for assertion in tests.
    pub struct VecEventSink {
        pub events: Mutex<Vec<AgentEventEnvelope>>,
        pub completes: Mutex<Vec<AgentCompletePayload>>,
    }

    impl VecEventSink {
        pub fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                completes: Mutex::new(Vec::new()),
            }
        }
    }

    impl Default for VecEventSink {
        fn default() -> Self {
            Self::new()
        }
    }

    impl EventSink for VecEventSink {
        fn emit_event(&self, envelope: &AgentEventEnvelope) {
            if let Ok(mut guard) = self.events.lock() {
                guard.push(envelope.clone());
            }
        }

        fn emit_complete(&self, payload: &AgentCompletePayload) {
            if let Ok(mut guard) = self.completes.lock() {
                guard.push(payload.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AgentEventPayload, AgentStatusEvent};

    #[test]
    fn null_sink_does_not_panic() {
        let sink = NullEventSink;
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "init".to_string(),
                message: "ok".to_string(),
            }),
        });
        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "done".to_string(),
        });
    }

    #[test]
    fn vec_sink_collects_events() {
        use test_util::VecEventSink;

        let sink = VecEventSink::new();
        sink.emit_event(&AgentEventEnvelope {
            tab_id: "t1".to_string(),
            payload: AgentEventPayload::Status(AgentStatusEvent {
                stage: "init".to_string(),
                message: "hello".to_string(),
            }),
        });
        sink.emit_complete(&AgentCompletePayload {
            tab_id: "t1".to_string(),
            outcome: "success".to_string(),
        });

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tab_id, "t1");

        let completes = sink.completes.lock().unwrap();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0].outcome, "success");
    }
}
