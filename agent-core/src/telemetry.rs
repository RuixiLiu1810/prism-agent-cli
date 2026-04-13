use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::provider::AgentTurnDescriptor;
use crate::session::AgentRuntimeState;
use crate::tools::AgentToolResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionRecord {
    pub event_type: String,
    pub recorded_at: String,
    pub tab_id: String,
    pub local_session_id: Option<String>,
    pub project_path: String,
    pub tool_name: String,
    pub target_label: Option<String>,
    pub success: bool,
    pub error_kind: Option<String>,
    pub approval_required: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentQuestionMetricsRecord {
    pub event_type: String,
    pub recorded_at: String,
    pub tab_id: String,
    pub local_session_id: Option<String>,
    pub project_path: String,
    pub outcome: String,
    pub doc_tool_rounds: u32,
    pub doc_tool_calls: u32,
    pub artifact_miss_count: u32,
    pub artifact_miss_rate: f64,
    pub fallback_count: u32,
    pub fallback_rate: f64,
    pub end_to_end_latency_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionTimer {
    started_at: Instant,
}

impl ToolExecutionTimer {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

fn classify_tool_error(result: &AgentToolResult) -> Option<String> {
    if !result.is_error {
        return None;
    }

    if result
        .content
        .get("approvalRequired")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Some("approval_required".to_string());
    }

    let error = result
        .content
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    if error.contains("not found verbatim") || error.contains("expected text was not found") {
        Some("not_found".to_string())
    } else if error.contains("matched") && error.contains("locations") {
        Some("ambiguous_match".to_string())
    } else if error.contains("cancelled") {
        Some("cancelled".to_string())
    } else if error.contains("Invalid tool arguments JSON") {
        Some("invalid_arguments".to_string())
    } else if error.is_empty() {
        Some("unknown".to_string())
    } else {
        Some("execution_error".to_string())
    }
}

pub async fn record_tool_execution(
    runtime_state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    result: &AgentToolResult,
    target_label: Option<String>,
    elapsed: Duration,
) {
    let record = ToolExecutionRecord {
        event_type: "tool_execution".to_string(),
        recorded_at: Utc::now().to_rfc3339(),
        tab_id: request.tab_id.clone(),
        local_session_id: request.local_session_id.clone(),
        project_path: request.project_path.clone(),
        tool_name: result.tool_name.clone(),
        target_label,
        success: !result.is_error,
        error_kind: classify_tool_error(result),
        approval_required: result
            .content
            .get("approvalRequired")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        duration_ms: elapsed.as_millis() as u64,
    };

    append_telemetry_record(runtime_state, &record).await;
}

pub fn document_artifact_miss(result: &AgentToolResult) -> bool {
    if let Some(status) = result
        .content
        .get("extractionStatus")
        .and_then(|value| value.as_str())
    {
        if status == "missing_artifact" {
            return true;
        }
    }
    let error = result
        .content
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    error.contains("No ingested document artifact is available")
}

pub fn document_fallback_used(result: &AgentToolResult) -> bool {
    result
        .content
        .get("fallbackUsed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub async fn record_document_question_metrics(
    runtime_state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    outcome: &str,
    doc_tool_rounds: u32,
    doc_tool_calls: u32,
    artifact_miss_count: u32,
    fallback_count: u32,
    end_to_end_latency: Duration,
) {
    let (artifact_miss_rate, fallback_rate) = if doc_tool_calls > 0 {
        (
            artifact_miss_count as f64 / doc_tool_calls as f64,
            fallback_count as f64 / doc_tool_calls as f64,
        )
    } else {
        (0.0, 0.0)
    };

    let record = DocumentQuestionMetricsRecord {
        event_type: "document_question_metrics".to_string(),
        recorded_at: Utc::now().to_rfc3339(),
        tab_id: request.tab_id.clone(),
        local_session_id: request.local_session_id.clone(),
        project_path: request.project_path.clone(),
        outcome: outcome.to_string(),
        doc_tool_rounds,
        doc_tool_calls,
        artifact_miss_count,
        artifact_miss_rate,
        fallback_count,
        fallback_rate,
        end_to_end_latency_ms: end_to_end_latency.as_millis() as u64,
    };

    append_telemetry_record(runtime_state, &record).await;
}

async fn append_telemetry_record<T: Serialize>(runtime_state: &AgentRuntimeState, record: &T) {
    let Some(path) = runtime_state.telemetry_log_path().await else {
        return;
    };

    let line = match serde_json::to_string(record) {
        Ok(line) => line,
        Err(err) => {
            eprintln!(
                "[agent][telemetry] failed to serialize tool record: {}",
                err
            );
            return;
        }
    };

    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(mut file) => {
            if let Err(err) = file.write_all(format!("{}\n", line).as_bytes()).await {
                eprintln!(
                    "[agent][telemetry] failed to append {}: {}",
                    path.display(),
                    err
                );
            }
        }
        Err(err) => {
            eprintln!(
                "[agent][telemetry] failed to open {}: {}",
                path.display(),
                err
            );
        }
    }
}
