use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub const CALLCHAIN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallSpanType {
    Turn,
    ProviderRound,
    ToolBatch,
    ToolCall,
    ApprovalSuspend,
    TurnResume,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallSpanStatus {
    Running,
    Ok,
    Error,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSpanEvent {
    pub name: String,
    pub at: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSpan {
    pub id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub span_type: CallSpanType,
    pub name: String,
    pub status: CallSpanStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    #[serde(default)]
    pub attrs: Value,
    #[serde(default)]
    pub events: Vec<CallSpanEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTraceStats {
    pub duration_ms: u64,
    pub tool_call_count: u32,
    pub approval_suspend_count: u32,
    pub retry_count: u32,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTrace {
    pub schema_version: u32,
    pub trace_id: String,
    pub tab_id: String,
    pub local_session_id: Option<String>,
    pub project_path: String,
    pub provider: String,
    pub model: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub outcome: Option<String>,
    #[serde(default)]
    pub spans: Vec<CallSpan>,
    #[serde(default)]
    pub stats: CallTraceStats,
}

impl CallTrace {
    pub fn new(
        tab_id: &str,
        local_session_id: Option<&str>,
        project_path: &str,
        provider: &str,
        model: &str,
    ) -> Self {
        Self {
            schema_version: CALLCHAIN_SCHEMA_VERSION,
            trace_id: Uuid::new_v4().to_string(),
            tab_id: tab_id.to_string(),
            local_session_id: local_session_id.map(str::to_string),
            project_path: project_path.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            outcome: None,
            spans: Vec::new(),
            stats: CallTraceStats::default(),
        }
    }

    pub fn start_span(
        &mut self,
        span_type: CallSpanType,
        name: impl Into<String>,
        parent_span_id: Option<String>,
        attrs: Value,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        self.spans.push(CallSpan {
            id: id.clone(),
            trace_id: self.trace_id.clone(),
            parent_span_id,
            span_type,
            name: name.into(),
            status: CallSpanStatus::Running,
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            attrs,
            events: Vec::new(),
        });
        id
    }

    pub fn add_event(&mut self, span_id: &str, name: impl Into<String>, payload: Value) -> bool {
        let Some(span) = self.spans.iter_mut().find(|span| span.id == span_id) else {
            return false;
        };
        span.events.push(CallSpanEvent {
            name: name.into(),
            at: Utc::now().to_rfc3339(),
            payload,
        });
        true
    }

    pub fn close_span(&mut self, span_id: &str, status: CallSpanStatus, attrs: Option<Value>) -> bool {
        let Some(span) = self.spans.iter_mut().find(|span| span.id == span_id) else {
            return false;
        };
        if span.ended_at.is_some() {
            return true;
        }
        span.status = status;
        span.ended_at = Some(Utc::now().to_rfc3339());
        if let Some(extra_attrs) = attrs {
            merge_json_object(&mut span.attrs, extra_attrs);
        }
        true
    }

    pub fn finalize(&mut self, outcome: &str) {
        for span in &mut self.spans {
            if span.ended_at.is_none() {
                span.status = CallSpanStatus::Interrupted;
                span.ended_at = Some(Utc::now().to_rfc3339());
            }
        }
        self.ended_at = Some(Utc::now().to_rfc3339());
        self.outcome = Some(outcome.to_string());
        self.recompute_stats();
    }

    pub fn recompute_stats(&mut self) {
        let mut stats = CallTraceStats::default();
        for span in &self.spans {
            match span.span_type {
                CallSpanType::ToolCall => {
                    stats.tool_call_count = stats.tool_call_count.saturating_add(1);
                }
                CallSpanType::ApprovalSuspend => {
                    stats.approval_suspend_count = stats.approval_suspend_count.saturating_add(1);
                }
                CallSpanType::Error => {
                    stats.error_count = stats.error_count.saturating_add(1);
                }
                _ => {}
            }
            if let Some(retry) = span.attrs.get("retry_count").and_then(Value::as_u64) {
                stats.retry_count = stats.retry_count.saturating_add(retry as u32);
            }
            if span.status == CallSpanStatus::Error {
                stats.error_count = stats.error_count.saturating_add(1);
            }
        }

        if let (Ok(start), Some(end)) = (
            chrono::DateTime::parse_from_rfc3339(&self.started_at),
            self.ended_at
                .as_deref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok()),
        ) {
            let millis = (end - start).num_milliseconds().max(0) as u64;
            stats.duration_ms = millis;
        }

        self.stats = stats;
    }

    pub fn to_export_value(&self) -> Value {
        json!(self)
    }
}

fn merge_json_object(target: &mut Value, source: Value) {
    if !target.is_object() {
        *target = json!({});
    }
    if let Some(source_obj) = source.as_object() {
        for (key, value) in source_obj {
            target[key] = value.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_parent_link_round_trips() {
        let mut trace = CallTrace::new("tab-1", Some("s1"), "/tmp/p", "minimax", "M2.7");
        let root = trace.start_span(CallSpanType::Turn, "turn", None, json!({}));
        let child = trace.start_span(
            CallSpanType::ToolCall,
            "read_file",
            Some(root.clone()),
            json!({"tool":"read_file"}),
        );
        assert!(trace.close_span(&child, CallSpanStatus::Ok, None));
        assert!(trace
            .spans
            .iter()
            .any(|span| span.id == child && span.parent_span_id.as_deref() == Some(root.as_str())));
    }

    #[test]
    fn finalize_marks_running_spans_interrupted() {
        let mut trace = CallTrace::new("tab-1", None, ".", "openai", "gpt");
        let running = trace.start_span(CallSpanType::ProviderRound, "round-1", None, json!({}));
        trace.finalize("completed");

        let span = trace
            .spans
            .iter()
            .find(|span| span.id == running)
            .expect("span exists");
        assert_eq!(span.status, CallSpanStatus::Interrupted);
        assert!(span.ended_at.is_some());
        assert_eq!(trace.outcome.as_deref(), Some("completed"));
    }

    #[test]
    fn recompute_stats_counts_tools_retries_and_errors() {
        let mut trace = CallTrace::new("tab", None, ".", "minimax", "M1");
        let tool_span = trace.start_span(CallSpanType::ToolCall, "tool", None, json!({}));
        let retry_span = trace.start_span(
            CallSpanType::ProviderRound,
            "round",
            None,
            json!({"retry_count": 2}),
        );
        let err_span = trace.start_span(CallSpanType::Error, "error", None, json!({}));
        trace.close_span(&tool_span, CallSpanStatus::Ok, None);
        trace.close_span(&retry_span, CallSpanStatus::Ok, None);
        trace.close_span(&err_span, CallSpanStatus::Error, None);
        trace.finalize("error");

        assert_eq!(trace.stats.tool_call_count, 1);
        assert_eq!(trace.stats.retry_count, 2);
        assert!(trace.stats.error_count >= 1);
    }
}
