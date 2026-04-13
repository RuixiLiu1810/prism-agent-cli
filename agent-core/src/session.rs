use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

use crate::provider::AgentTurnProfile;
use crate::workflows::AgentWorkflowState;

const PENDING_TURN_TTL_MINUTES: i64 = 10;
const ALLOW_ONCE_TTL_MINUTES: i64 = 15;
const AGENT_RUNTIME_DIR: &str = "agent-runtime";
const PENDING_TURNS_FILE: &str = "pending-turns.json";
const TOOL_APPROVALS_FILE: &str = "tool-approvals.json";
const TOOL_EXECUTION_LOG_FILE: &str = "tool-execution.jsonl";
const WORKFLOW_STATES_FILE: &str = "workflow-states.json";
const SESSION_WORK_STATES_FILE: &str = "session-work-states.json";

const MEMORY_DIR: &str = "memory";
const MEMORY_INDEX_FILE: &str = "index.json";
const MEMORY_MAX_ENTRIES: usize = 50;
const MEMORY_INJECTION_TOKEN_BUDGET: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    UserPreference,
    ProjectConvention,
    Correction,
    Reference,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::UserPreference => write!(f, "preference"),
            MemoryType::ProjectConvention => write!(f, "convention"),
            MemoryType::Correction => write!(f, "correction"),
            MemoryType::Reference => write!(f, "reference"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub topic: Option<String>,
    pub source_session: Option<String>,
    pub created_at: String,
    pub last_accessed: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryIndex {
    #[serde(default)]
    pub entries: Vec<MemoryEntry>,
    #[serde(default)]
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRecord {
    pub local_session_id: String,
    pub provider: String,
    pub project_path: String,
    pub tab_id: String,
    pub title: String,
    pub model: String,
    pub previous_response_id: Option<String>,
    pub last_response_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentSessionRecord {
    #[allow(dead_code)]
    pub fn new(
        provider: &str,
        project_path: String,
        tab_id: String,
        title: String,
        model: String,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            local_session_id: Uuid::new_v4().to_string(),
            provider: provider.to_string(),
            project_path,
            tab_id,
            title,
            model,
            previous_response_id: None,
            last_response_id: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[allow(dead_code)]
    pub fn touch_response(&mut self, response_id: Option<String>) {
        self.previous_response_id = self.last_response_id.clone();
        self.last_response_id = response_id;
        self.updated_at = Utc::now().to_rfc3339();
    }
}

pub type AgentHistoryItem = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSummary {
    pub local_session_id: String,
    pub title: String,
    pub updated_at: String,
    pub created_at: String,
    pub provider: String,
    pub model: String,
    pub preview: Option<String>,
    pub message_count: usize,
    pub current_objective: Option<String>,
    pub current_target: Option<String>,
    pub last_tool_activity: Option<String>,
    pub pending_state: Option<String>,
    pub pending_target: Option<String>,
    pub workflow_type: Option<String>,
    pub workflow_stage: Option<String>,
    pub collected_reference_count: usize,
    pub review_finding_count: usize,
    pub has_revision_tracker: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    AllowSession,
    AllowOnce,
    DenySession,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRecord {
    pub decision: ToolApprovalDecision,
    pub source: String,
    pub granted_at: String,
    pub expires_at: Option<String>,
    pub remaining_uses: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ToolApprovalState {
    pub allow_session: bool,
    pub deny_session: bool,
    pub allow_once_remaining: u32,
    #[allow(dead_code)]
    pub source: Option<String>,
    #[allow(dead_code)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionWorkState {
    pub current_objective: Option<String>,
    pub recent_objective: Option<String>,
    pub current_target: Option<String>,
    pub last_tool_activity: Option<String>,
    pub pending_state: Option<String>,
    pub pending_tool_name: Option<String>,
    pub pending_target: Option<String>,
    #[serde(default)]
    pub collected_references: Vec<CollectedReference>,
    #[serde(default)]
    pub academic_workflow: AcademicWorkflowSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectedReference {
    pub doi: Option<String>,
    pub pmid: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub journal: Option<String>,
    pub abstract_text: Option<String>,
    pub user_notes: Option<String>,
    pub relevance_tag: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademicWorkflowSnapshot {
    pub workflow_type: Option<String>,
    pub current_step: Option<String>,
    pub manuscript_outline: Option<ManuscriptOutlineSnapshot>,
    #[serde(default)]
    pub review_findings: Vec<StoredReviewFinding>,
    pub revision_tracker: Option<RevisionTrackerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptOutlineSnapshot {
    pub sections: Vec<String>,
    pub source_tool: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredReviewFinding {
    pub severity: Option<String>,
    pub dimension: Option<String>,
    pub message: String,
    pub suggestion: Option<String>,
    pub source_tool: String,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionTrackerSnapshot {
    pub changed_line_count: Option<u32>,
    pub first_changed_line: Option<u32>,
    pub old_word_count: Option<u32>,
    pub new_word_count: Option<u32>,
    pub delta_word_count: Option<i32>,
    pub summary: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTurnResume {
    pub project_path: String,
    pub tab_id: String,
    pub local_session_id: Option<String>,
    pub model: Option<String>,
    pub turn_profile: Option<AgentTurnProfile>,
    pub approval_tool_name: String,
    pub target_label: Option<String>,
    pub continuation_prompt: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub expires_at: String,
}

#[derive(Clone)]
pub struct AgentRuntimeState {
    pub sessions: Arc<Mutex<HashMap<String, AgentSessionRecord>>>,
    pub histories: Arc<Mutex<HashMap<String, Vec<AgentHistoryItem>>>>,
    pub cancellations: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    pub tool_approvals: Arc<Mutex<HashMap<String, HashMap<String, ToolApprovalRecord>>>>,
    pub pending_turns: Arc<Mutex<HashMap<String, PendingTurnResume>>>,
    pub tab_work_states: Arc<Mutex<HashMap<String, AgentSessionWorkState>>>,
    pub session_work_states: Arc<Mutex<HashMap<String, AgentSessionWorkState>>>,
    pub workflows: Arc<Mutex<HashMap<String, AgentWorkflowState>>>,
    pub memory_index: Arc<Mutex<MemoryIndex>>,
    pub active_turns: Arc<Mutex<HashSet<String>>>,
    persistence_dir: Arc<Mutex<Option<PathBuf>>>,
    persistence_loaded: Arc<Mutex<bool>>,
    telemetry_log_path: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            histories: Arc::new(Mutex::new(HashMap::new())),
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            tool_approvals: Arc::new(Mutex::new(HashMap::new())),
            pending_turns: Arc::new(Mutex::new(HashMap::new())),
            tab_work_states: Arc::new(Mutex::new(HashMap::new())),
            session_work_states: Arc::new(Mutex::new(HashMap::new())),
            workflows: Arc::new(Mutex::new(HashMap::new())),
            memory_index: Arc::new(Mutex::new(MemoryIndex::default())),
            active_turns: Arc::new(Mutex::new(HashSet::new())),
            persistence_dir: Arc::new(Mutex::new(None)),
            persistence_loaded: Arc::new(Mutex::new(false)),
            telemetry_log_path: Arc::new(Mutex::new(None)),
        }
    }
}

impl AgentRuntimeState {
    pub async fn ensure_storage_at(&self, app_config_dir: PathBuf) -> Result<(), String> {
        let already_loaded = { *self.persistence_loaded.lock().await };
        if already_loaded {
            return Ok(());
        }

        let base_dir = app_config_dir.join(AGENT_RUNTIME_DIR);
        tokio::fs::create_dir_all(&base_dir)
            .await
            .map_err(|err| format!("Failed to create agent runtime dir: {}", err))?;

        {
            let mut persistence_dir = self.persistence_dir.lock().await;
            *persistence_dir = Some(base_dir.clone());
        }
        {
            let mut telemetry_path = self.telemetry_log_path.lock().await;
            *telemetry_path = Some(base_dir.join(TOOL_EXECUTION_LOG_FILE));
        }

        let approvals_path = base_dir.join(TOOL_APPROVALS_FILE);
        let pending_path = base_dir.join(PENDING_TURNS_FILE);
        let workflows_path = base_dir.join(WORKFLOW_STATES_FILE);
        let session_work_states_path = base_dir.join(SESSION_WORK_STATES_FILE);
        let memory_dir = base_dir.join(MEMORY_DIR);

        let mut approvals =
            read_json_file::<HashMap<String, HashMap<String, ToolApprovalRecord>>>(&approvals_path)
                .await?;
        let mut pending_turns =
            read_json_file::<HashMap<String, PendingTurnResume>>(&pending_path).await?;
        let workflows =
            read_json_file::<HashMap<String, AgentWorkflowState>>(&workflows_path).await?;
        let session_work_states =
            read_json_file::<HashMap<String, AgentSessionWorkState>>(&session_work_states_path)
                .await?;

        let memory_index = load_memory_index(&memory_dir).await;

        let approvals_dirty = cleanup_expired_tool_approvals(&mut approvals);
        let pending_dirty = cleanup_expired_pending_turns(&mut pending_turns);

        {
            let mut approval_state = self.tool_approvals.lock().await;
            *approval_state = approvals;
        }
        {
            let mut pending_state = self.pending_turns.lock().await;
            *pending_state = pending_turns;
        }
        {
            let mut workflow_state = self.workflows.lock().await;
            *workflow_state = workflows;
        }
        {
            let mut session_states = self.session_work_states.lock().await;
            *session_states = session_work_states;
        }
        {
            let mut mem = self.memory_index.lock().await;
            *mem = memory_index;
        }

        {
            let mut loaded = self.persistence_loaded.lock().await;
            *loaded = true;
        }

        if approvals_dirty {
            self.persist_tool_approvals().await?;
        }
        if pending_dirty {
            self.persist_pending_turns().await?;
        }

        Ok(())
    }

    pub async fn telemetry_log_path(&self) -> Option<PathBuf> {
        self.telemetry_log_path.lock().await.clone()
    }

    pub async fn list_session_summaries_for_project(
        &self,
        project_path: &str,
    ) -> Vec<AgentSessionSummary> {
        let records = self.list_sessions_for_project(project_path).await;
        let history_summaries = {
            let histories = self.histories.lock().await;
            records
                .iter()
                .map(|session| {
                    let history = histories.get(&session.local_session_id);
                    (
                        history.and_then(|items| summarize_history_preview(items)),
                        history.map(|items| items.len()).unwrap_or(0),
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut summaries = Vec::with_capacity(records.len());

        for (session, (preview, message_count)) in records.into_iter().zip(history_summaries) {
            let work_state = self
                .work_state_for_summary(&session.local_session_id, &session.tab_id)
                .await;

            summaries.push(AgentSessionSummary {
                local_session_id: session.local_session_id,
                title: session.title,
                updated_at: session.updated_at,
                created_at: session.created_at,
                provider: session.provider,
                model: session.model,
                preview,
                message_count,
                current_objective: work_state.current_objective,
                current_target: work_state.current_target,
                last_tool_activity: work_state.last_tool_activity,
                pending_state: work_state.pending_state,
                pending_target: work_state.pending_target,
                workflow_type: work_state.academic_workflow.workflow_type.clone(),
                workflow_stage: work_state.academic_workflow.current_step.clone(),
                collected_reference_count: work_state.collected_references.len(),
                review_finding_count: work_state.academic_workflow.review_findings.len(),
                has_revision_tracker: work_state.academic_workflow.revision_tracker.is_some(),
            });
        }

        summaries
    }

    pub async fn list_sessions_for_project(&self, project_path: &str) -> Vec<AgentSessionRecord> {
        let sessions = self.sessions.lock().await;
        let mut records = sessions
            .values()
            .filter(|session| session.project_path == project_path)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        records
    }

    pub async fn session_summary(&self, local_session_id: &str) -> Option<AgentSessionSummary> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(local_session_id)?.clone();
        drop(sessions);

        let histories = self.histories.lock().await;
        let history = histories.get(local_session_id);
        let preview = history.and_then(|items| summarize_history_preview(items));
        let message_count = history.map(|items| items.len()).unwrap_or(0);
        drop(histories);
        let work_state = self
            .work_state_for_summary(local_session_id, &session.tab_id)
            .await;

        Some(AgentSessionSummary {
            local_session_id: session.local_session_id,
            title: session.title,
            updated_at: session.updated_at,
            created_at: session.created_at,
            provider: session.provider,
            model: session.model,
            preview,
            message_count,
            current_objective: work_state.current_objective,
            current_target: work_state.current_target,
            last_tool_activity: work_state.last_tool_activity,
            pending_state: work_state.pending_state,
            pending_target: work_state.pending_target,
            workflow_type: work_state.academic_workflow.workflow_type.clone(),
            workflow_stage: work_state.academic_workflow.current_step.clone(),
            collected_reference_count: work_state.collected_references.len(),
            review_finding_count: work_state.academic_workflow.review_findings.len(),
            has_revision_tracker: work_state.academic_workflow.revision_tracker.is_some(),
        })
    }

    pub async fn history_for_session(
        &self,
        local_session_id: &str,
    ) -> Option<Vec<AgentHistoryItem>> {
        let histories = self.histories.lock().await;
        histories.get(local_session_id).cloned()
    }

    pub async fn append_history(&self, local_session_id: &str, items: Vec<AgentHistoryItem>) {
        if items.is_empty() {
            return;
        }
        let mut histories = self.histories.lock().await;
        histories
            .entry(local_session_id.to_string())
            .or_default()
            .extend(items);
    }

    pub async fn register_cancellation(&self, tab_id: &str) -> watch::Receiver<bool> {
        let (sender, receiver) = watch::channel(false);
        let mut cancellations = self.cancellations.lock().await;
        cancellations.insert(tab_id.to_string(), sender);
        receiver
    }

    pub async fn cancel_tab(&self, tab_id: &str) -> bool {
        let cancellations = self.cancellations.lock().await;
        if let Some(sender) = cancellations.get(tab_id) {
            let _ = sender.send(true);
            true
        } else {
            false
        }
    }

    pub async fn clear_cancellation(&self, tab_id: &str) {
        let mut cancellations = self.cancellations.lock().await;
        cancellations.remove(tab_id);
    }

    pub async fn acquire_turn_guard(&self, tab_id: &str) -> Result<(), String> {
        let mut active = self.active_turns.lock().await;
        if active.contains(tab_id) {
            return Err(format!(
                "A turn is already running on tab '{}'. Cancel it or wait for completion.",
                tab_id
            ));
        }
        active.insert(tab_id.to_string());
        Ok(())
    }

    pub async fn release_turn_guard(&self, tab_id: &str) {
        let mut active = self.active_turns.lock().await;
        active.remove(tab_id);
    }

    pub async fn set_tool_approval(
        &self,
        tab_id: &str,
        tool_name: &str,
        decision: &str,
    ) -> Result<(), String> {
        let mut approvals = self.tool_approvals.lock().await;
        let tab_entry = approvals.entry(tab_id.to_string()).or_default();
        let now = Utc::now();

        match decision {
            "allow_once" => {
                tab_entry.insert(
                    tool_name.to_string(),
                    ToolApprovalRecord {
                        decision: ToolApprovalDecision::AllowOnce,
                        source: "user".to_string(),
                        granted_at: now.to_rfc3339(),
                        expires_at: Some(
                            (now + Duration::minutes(ALLOW_ONCE_TTL_MINUTES)).to_rfc3339(),
                        ),
                        remaining_uses: 1,
                    },
                );
            }
            "allow_session" => {
                tab_entry.insert(
                    tool_name.to_string(),
                    ToolApprovalRecord {
                        decision: ToolApprovalDecision::AllowSession,
                        source: "user".to_string(),
                        granted_at: now.to_rfc3339(),
                        expires_at: None,
                        remaining_uses: 0,
                    },
                );
            }
            "deny_session" => {
                tab_entry.insert(
                    tool_name.to_string(),
                    ToolApprovalRecord {
                        decision: ToolApprovalDecision::DenySession,
                        source: "user".to_string(),
                        granted_at: now.to_rfc3339(),
                        expires_at: None,
                        remaining_uses: 0,
                    },
                );
            }
            other => {
                return Err(format!("Unsupported tool approval decision: {}", other));
            }
        }

        drop(approvals);
        self.persist_tool_approvals().await?;
        Ok(())
    }

    pub async fn clear_tool_approvals(&self, tab_id: &str) {
        let mut approvals = self.tool_approvals.lock().await;
        approvals.remove(tab_id);
        drop(approvals);
        let _ = self.persist_tool_approvals().await;
    }

    pub async fn check_tool_approval(&self, tab_id: &str, tool_name: &str) -> ToolApprovalState {
        let mut approvals = self.tool_approvals.lock().await;
        let tab_entry = approvals.entry(tab_id.to_string()).or_default();
        let expired = tab_entry
            .get(tool_name)
            .map(record_is_expired)
            .unwrap_or(false);
        if expired {
            tab_entry.remove(tool_name);
        }

        let mut persist = expired;
        let snapshot = if let Some(record) = tab_entry.get(tool_name).cloned() {
            let mut state = ToolApprovalState {
                source: Some(record.source.clone()),
                expires_at: record.expires_at.clone(),
                ..Default::default()
            };
            match record.decision {
                ToolApprovalDecision::AllowSession => {
                    state.allow_session = true;
                }
                ToolApprovalDecision::DenySession => {
                    state.deny_session = true;
                }
                ToolApprovalDecision::AllowOnce => {
                    if record.remaining_uses > 0 {
                        state.allow_once_remaining = record.remaining_uses;
                        if let Some(existing) = tab_entry.get_mut(tool_name) {
                            existing.remaining_uses = existing.remaining_uses.saturating_sub(1);
                            if existing.remaining_uses == 0 {
                                tab_entry.remove(tool_name);
                            }
                        }
                        persist = true;
                    }
                }
            }
            state
        } else {
            ToolApprovalState::default()
        };

        drop(approvals);
        if persist {
            let _ = self.persist_tool_approvals().await;
        }
        snapshot
    }

    pub async fn set_current_objective(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        objective: Option<String>,
    ) {
        let normalized_objective = objective
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.update_work_state(tab_id, local_session_id, |state| {
            if state.current_objective != normalized_objective {
                if let Some(previous) = state.current_objective.clone() {
                    if Some(previous.clone()) != normalized_objective {
                        state.recent_objective = Some(previous);
                    }
                }
                state.current_objective = normalized_objective.clone();
            }
        })
        .await;
    }

    pub async fn record_tool_running(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        tool_name: &str,
        target: Option<&str>,
    ) {
        let tool_activity = if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
            format!("Running {} on {}", tool_name, target)
        } else {
            format!("Running {}", tool_name)
        };
        self.update_work_state(tab_id, local_session_id, |state| {
            if target.is_some() {
                state.current_target = target.map(str::to_string);
            }
            state.last_tool_activity = Some(tool_activity.clone());
        })
        .await;
    }

    pub async fn record_tool_result(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        tool_name: &str,
        target: Option<&str>,
        is_error: bool,
    ) {
        let prefix = if is_error { "Result" } else { "Completed" };
        let tool_activity = if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
            format!("{} {} on {}", prefix, tool_name, target)
        } else {
            format!("{} {}", prefix, tool_name)
        };
        self.update_work_state(tab_id, local_session_id, |state| {
            if target.is_some() {
                state.current_target = target.map(str::to_string);
            }
            state.last_tool_activity = Some(tool_activity.clone());
        })
        .await;
    }

    pub async fn record_collected_references_from_tool_result(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        tool_name: &str,
        content: &Value,
    ) {
        if tool_name != "search_literature" {
            return;
        }
        let Some(results) = content.get("results").and_then(Value::as_array) else {
            return;
        };
        if results.is_empty() {
            return;
        }

        let added_at = now_rfc3339();
        let parsed = results
            .iter()
            .filter_map(|item| parse_collected_reference(item, &added_at))
            .collect::<Vec<_>>();
        if parsed.is_empty() {
            return;
        }

        self.update_work_state(tab_id, local_session_id, |state| {
            for candidate in &parsed {
                if let Some(existing) = state
                    .collected_references
                    .iter_mut()
                    .find(|existing| same_reference(existing, candidate))
                {
                    if existing.abstract_text.is_none() && candidate.abstract_text.is_some() {
                        existing.abstract_text = candidate.abstract_text.clone();
                    }
                    if existing.journal.is_none() && candidate.journal.is_some() {
                        existing.journal = candidate.journal.clone();
                    }
                    if existing.year.is_none() && candidate.year.is_some() {
                        existing.year = candidate.year;
                    }
                    continue;
                }
                state.collected_references.push(candidate.clone());
            }
            if state.collected_references.len() > 200 {
                let drop_count = state.collected_references.len() - 200;
                state.collected_references.drain(0..drop_count);
            }
        })
        .await;
    }

    pub async fn sync_workflow_snapshot(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        workflow_type: Option<&str>,
        current_step: Option<&str>,
    ) {
        self.update_work_state(tab_id, local_session_id, |state| {
            state.academic_workflow.workflow_type = workflow_type
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            state.academic_workflow.current_step = current_step
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        })
        .await;
    }

    pub async fn record_academic_artifacts_from_tool_result(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        tool_name: &str,
        content: &Value,
    ) {
        let captured_at = now_rfc3339();
        self.update_work_state(tab_id, local_session_id, |state| match tool_name {
            "restructure_outline" => {
                let sections = extract_outline_sections(content);
                if sections.is_empty() {
                    return;
                }
                state.academic_workflow.manuscript_outline = Some(ManuscriptOutlineSnapshot {
                    sections,
                    source_tool: tool_name.to_string(),
                    updated_at: captured_at.clone(),
                });
            }
            "review_manuscript" | "check_statistics" | "check_consistency" => {
                let findings = extract_review_findings(content, tool_name, &captured_at);
                if findings.is_empty() {
                    return;
                }
                for finding in findings {
                    if state
                        .academic_workflow
                        .review_findings
                        .iter()
                        .any(|existing| same_stored_review_finding(existing, &finding))
                    {
                        continue;
                    }
                    state.academic_workflow.review_findings.push(finding);
                }
                if state.academic_workflow.review_findings.len() > 200 {
                    let drop_count = state.academic_workflow.review_findings.len() - 200;
                    state.academic_workflow.review_findings.drain(0..drop_count);
                }
            }
            "track_revisions" => {
                if let Some(snapshot) = extract_revision_tracker(content, &captured_at) {
                    state.academic_workflow.revision_tracker = Some(snapshot);
                }
            }
            _ => {}
        })
        .await;
    }

    pub async fn collected_references_for(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
    ) -> Vec<CollectedReference> {
        let state = self.work_state_for_prompt(tab_id, local_session_id).await;
        state.collected_references
    }

    pub async fn update_collected_reference(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        doi: Option<String>,
        pmid: Option<String>,
        title: Option<String>,
        user_notes: Option<String>,
        relevance_tag: Option<String>,
    ) -> Result<(), String> {
        let doi = normalize_reference_id(doi.as_deref());
        let pmid = normalize_reference_id(pmid.as_deref());
        let title = title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if doi.is_none() && pmid.is_none() && title.is_none() {
            return Err(
                "Provide at least one reference identifier: doi, pmid, or title.".to_string(),
            );
        }

        let normalized_notes = user_notes
            .as_deref()
            .map(str::trim)
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty());
        let normalized_tag = normalize_relevance_tag(relevance_tag.as_deref())?;

        let mut updated = false;
        self.update_work_state(tab_id, local_session_id, |state| {
            let candidate = state.collected_references.iter_mut().find(|entry| {
                reference_matches_selector(entry, doi.as_deref(), pmid.as_deref(), title.as_deref())
            });
            if let Some(entry) = candidate {
                entry.user_notes = normalized_notes.clone();
                entry.relevance_tag = normalized_tag.clone();
                updated = true;
            }
        })
        .await;

        if updated {
            Ok(())
        } else {
            Err("No matching collected reference found for the provided identifier.".to_string())
        }
    }

    pub async fn clear_collected_references(&self, tab_id: &str, local_session_id: Option<&str>) {
        self.update_work_state(tab_id, local_session_id, |state| {
            state.collected_references.clear();
        })
        .await;
    }

    pub async fn mark_pending_state(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        pending_state: &str,
        tool_name: &str,
        target: Option<&str>,
    ) {
        self.update_work_state(tab_id, local_session_id, |state| {
            state.pending_state = Some(pending_state.to_string());
            state.pending_tool_name = Some(tool_name.to_string());
            state.pending_target = target.map(str::to_string);
            if target.is_some() {
                state.current_target = target.map(str::to_string);
            }
        })
        .await;
    }

    pub async fn clear_pending_state(&self, tab_id: &str, local_session_id: Option<&str>) {
        self.update_work_state(tab_id, local_session_id, |state| {
            state.pending_state = None;
            state.pending_tool_name = None;
            state.pending_target = None;
        })
        .await;
    }

    pub async fn store_pending_turn(&self, pending: PendingTurnResume) {
        let pending = pending_turn_with_ttl(pending);
        let mut pending_turns = self.pending_turns.lock().await;
        pending_turns.insert(pending.tab_id.clone(), pending);
        drop(pending_turns);
        let _ = self.persist_pending_turns().await;
    }

    pub async fn take_pending_turn(&self, tab_id: &str) -> Option<PendingTurnResume> {
        let mut pending_turns = self.pending_turns.lock().await;
        cleanup_expired_pending_turns(&mut pending_turns);
        let pending = pending_turns.remove(tab_id);
        drop(pending_turns);
        let _ = self.persist_pending_turns().await;
        pending
    }

    pub async fn bind_tab_state_to_session(&self, tab_id: &str, local_session_id: &str) {
        let maybe_tab_state = {
            let mut tab_work_states = self.tab_work_states.lock().await;
            tab_work_states.remove(tab_id)
        };

        if let Some(tab_state) = maybe_tab_state {
            let mut session_work_states = self.session_work_states.lock().await;
            session_work_states.insert(local_session_id.to_string(), tab_state);
        }
        let _ = self.persist_session_work_states().await;

        let mut pending_turns = self.pending_turns.lock().await;
        if let Some(pending) = pending_turns.get_mut(tab_id) {
            pending.local_session_id = Some(local_session_id.to_string());
        }
        drop(pending_turns);
        let _ = self.persist_pending_turns().await;
        self.bind_workflow_to_session(tab_id, Some(local_session_id))
            .await;
    }

    pub async fn clear_pending_turn(&self, tab_id: &str, local_session_id: Option<&str>) {
        {
            let mut pending_turns = self.pending_turns.lock().await;
            pending_turns.remove(tab_id);
        }
        let _ = self.persist_pending_turns().await;
        self.clear_pending_state(tab_id, local_session_id).await;
    }

    pub async fn upsert_workflow_state(&self, workflow: AgentWorkflowState) {
        let workflow_type = workflow.workflow_type.as_str().to_string();
        let current_stage = workflow.current_stage.clone();
        let workflow_tab_id = workflow.tab_id.clone();
        let workflow_session_id = workflow.local_session_id.clone();
        let workflow_id = workflow.workflow_id.clone();
        {
            let mut workflows = self.workflows.lock().await;
            workflows.insert(workflow_id, workflow);
        }
        let _ = self.persist_workflow_states().await;
        self.sync_workflow_snapshot(
            &workflow_tab_id,
            workflow_session_id.as_deref(),
            Some(&workflow_type),
            Some(&current_stage),
        )
        .await;
    }

    pub async fn workflow_state_for(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
    ) -> Option<AgentWorkflowState> {
        let workflows = self.workflows.lock().await;
        workflows
            .values()
            .find(|workflow| workflow_matches(workflow, tab_id, local_session_id))
            .cloned()
    }

    pub async fn workflow_has_pending_checkpoint(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
    ) -> bool {
        self.workflow_state_for(tab_id, local_session_id)
            .await
            .map(|workflow| workflow.pending_checkpoint)
            .unwrap_or(false)
    }

    pub async fn bind_workflow_to_session(&self, tab_id: &str, local_session_id: Option<&str>) {
        let Some(local_session_id) = local_session_id.filter(|value| !value.trim().is_empty())
        else {
            return;
        };

        let mut changed = false;
        {
            let mut workflows = self.workflows.lock().await;
            for workflow in workflows.values_mut() {
                let matches_tab = workflow.tab_id == tab_id;
                let matches_session =
                    workflow.local_session_id.as_deref() == Some(local_session_id);
                if matches_tab || matches_session {
                    workflow.tab_id = tab_id.to_string();
                    workflow.bind_local_session_id(Some(local_session_id));
                    changed = true;
                }
            }
        }
        if changed {
            let _ = self.persist_workflow_states().await;
        }
    }

    pub async fn clear_workflow_state(&self, tab_id: &str, local_session_id: Option<&str>) {
        let mut changed = false;
        {
            let mut workflows = self.workflows.lock().await;
            let workflow_ids = workflows
                .iter()
                .filter_map(|(workflow_id, workflow)| {
                    if workflow_matches(workflow, tab_id, local_session_id) {
                        Some(workflow_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for workflow_id in workflow_ids {
                workflows.remove(&workflow_id);
                changed = true;
            }
        }
        if changed {
            let _ = self.persist_workflow_states().await;
            self.sync_workflow_snapshot(tab_id, local_session_id, None, None)
                .await;
        }
    }

    pub async fn work_state_for_prompt(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
    ) -> AgentSessionWorkState {
        if let Some(local_session_id) = local_session_id.filter(|value| !value.trim().is_empty()) {
            let session_work_states = self.session_work_states.lock().await;
            if let Some(state) = session_work_states.get(local_session_id) {
                return state.clone();
            }
        }

        let tab_work_states = self.tab_work_states.lock().await;
        tab_work_states.get(tab_id).cloned().unwrap_or_default()
    }

    async fn persist_tool_approvals(&self) -> Result<(), String> {
        let Some(path) = self.persistence_file(TOOL_APPROVALS_FILE).await else {
            return Ok(());
        };
        let approvals = {
            let mut approvals = self.tool_approvals.lock().await;
            cleanup_expired_tool_approvals(&mut approvals);
            approvals.clone()
        };
        write_json_file(&path, &approvals).await
    }

    async fn persist_pending_turns(&self) -> Result<(), String> {
        let Some(path) = self.persistence_file(PENDING_TURNS_FILE).await else {
            return Ok(());
        };
        let pending_turns = {
            let mut pending_turns = self.pending_turns.lock().await;
            cleanup_expired_pending_turns(&mut pending_turns);
            pending_turns.clone()
        };
        write_json_file(&path, &pending_turns).await
    }

    async fn persist_workflow_states(&self) -> Result<(), String> {
        let Some(path) = self.persistence_file(WORKFLOW_STATES_FILE).await else {
            return Ok(());
        };
        let workflows = {
            let workflows = self.workflows.lock().await;
            workflows.clone()
        };
        write_json_file(&path, &workflows).await
    }

    async fn persist_session_work_states(&self) -> Result<(), String> {
        let Some(path) = self.persistence_file(SESSION_WORK_STATES_FILE).await else {
            return Ok(());
        };
        let session_work_states = {
            let session_work_states = self.session_work_states.lock().await;
            session_work_states.clone()
        };
        write_json_file(&path, &session_work_states).await
    }

    async fn persistence_file(&self, file_name: &str) -> Option<PathBuf> {
        self.persistence_dir
            .lock()
            .await
            .clone()
            .map(|dir| dir.join(file_name))
    }

    async fn memory_dir(&self) -> Option<PathBuf> {
        self.persistence_dir
            .lock()
            .await
            .clone()
            .map(|dir| dir.join(MEMORY_DIR))
    }

    pub async fn save_memory_entry(&self, entry: MemoryEntry) -> Result<(), String> {
        let Some(dir) = self.memory_dir().await else {
            return Err("Memory persistence not initialized".to_string());
        };
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| format!("Failed to create memory dir: {}", e))?;

        let mut index = self.memory_index.lock().await;

        // Deduplicate: if same content already exists, update last_accessed only
        if let Some(existing) = index
            .entries
            .iter_mut()
            .find(|e| e.content == entry.content)
        {
            existing.last_accessed = Utc::now().to_rfc3339();
            let snapshot = index.clone();
            drop(index);
            write_json_file(&dir.join(MEMORY_INDEX_FILE), &snapshot).await?;
            return Ok(());
        }

        index.entries.push(entry);
        index.version += 1;

        // Enforce max entries: drop oldest by last_accessed
        if index.entries.len() > MEMORY_MAX_ENTRIES {
            index
                .entries
                .sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
            index.entries.truncate(MEMORY_MAX_ENTRIES);
        }

        let snapshot = index.clone();
        drop(index);

        write_json_file(&dir.join(MEMORY_INDEX_FILE), &snapshot).await
    }

    pub async fn build_memory_context(&self) -> String {
        let index = self.memory_index.lock().await;
        if index.entries.is_empty() {
            return String::new();
        }

        let mut sorted = index.entries.clone();
        sorted.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));

        let mut result = String::from("[Project Memory]\n");
        let mut token_budget = MEMORY_INJECTION_TOKEN_BUDGET;

        for entry in &sorted {
            let line = format!("- [{}] {}\n", entry.memory_type, entry.content);
            let line_tokens = estimate_memory_tokens(&line);
            if line_tokens > token_budget {
                break;
            }
            token_budget -= line_tokens;
            result.push_str(&line);
        }

        result
    }

    async fn update_work_state<F>(
        &self,
        tab_id: &str,
        local_session_id: Option<&str>,
        mut updater: F,
    ) where
        F: FnMut(&mut AgentSessionWorkState),
    {
        {
            let mut tab_work_states = self.tab_work_states.lock().await;
            let state = tab_work_states.entry(tab_id.to_string()).or_default();
            updater(state);
        }

        if let Some(local_session_id) = local_session_id.filter(|value| !value.trim().is_empty()) {
            let mut session_work_states = self.session_work_states.lock().await;
            let state = session_work_states
                .entry(local_session_id.to_string())
                .or_default();
            updater(state);
            drop(session_work_states);
            let _ = self.persist_session_work_states().await;
        }
    }

    async fn work_state_for_summary(
        &self,
        local_session_id: &str,
        tab_id: &str,
    ) -> AgentSessionWorkState {
        let session_work_states = self.session_work_states.lock().await;
        if let Some(state) = session_work_states.get(local_session_id) {
            return state.clone();
        }
        drop(session_work_states);

        let tab_work_states = self.tab_work_states.lock().await;
        tab_work_states.get(tab_id).cloned().unwrap_or_default()
    }
}

fn parse_collected_reference(value: &Value, added_at: &str) -> Option<CollectedReference> {
    let title = value
        .get("title")
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }

    let doi = value
        .get("doi")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string);
    let pmid = value
        .get("pmid")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string);
    let journal = value
        .get("journal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string);
    let abstract_text = value
        .get("abstract")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string);
    let year = value
        .get("year")
        .and_then(Value::as_u64)
        .and_then(|entry| u32::try_from(entry).ok());
    let authors = value
        .get("authors")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(CollectedReference {
        doi,
        pmid,
        title,
        authors,
        year,
        journal,
        abstract_text,
        user_notes: None,
        relevance_tag: None,
        added_at: added_at.to_string(),
    })
}

fn extract_outline_sections(content: &Value) -> Vec<String> {
    let Some(revised_outline) = content.get("revisedOutline").and_then(Value::as_array) else {
        return Vec::new();
    };
    revised_outline
        .iter()
        .filter_map(|item| item.get("section").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
}

fn extract_review_findings(
    content: &Value,
    source_tool: &str,
    captured_at: &str,
) -> Vec<StoredReviewFinding> {
    let Some(findings) = content.get("findings").and_then(Value::as_array) else {
        return Vec::new();
    };
    findings
        .iter()
        .filter_map(|item| {
            let message = item
                .get("message")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let severity = item
                .get("severity")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let dimension = item
                .get("dimension")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let suggestion = item
                .get("suggestion")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Some(StoredReviewFinding {
                severity,
                dimension,
                message,
                suggestion,
                source_tool: source_tool.to_string(),
                captured_at: captured_at.to_string(),
            })
        })
        .collect::<Vec<_>>()
}

fn same_stored_review_finding(left: &StoredReviewFinding, right: &StoredReviewFinding) -> bool {
    left.message.eq_ignore_ascii_case(&right.message)
        && left
            .dimension
            .as_deref()
            .unwrap_or_default()
            .eq_ignore_ascii_case(right.dimension.as_deref().unwrap_or_default())
        && left
            .severity
            .as_deref()
            .unwrap_or_default()
            .eq_ignore_ascii_case(right.severity.as_deref().unwrap_or_default())
}

fn value_to_u32(value: Option<u64>) -> Option<u32> {
    value.and_then(|entry| u32::try_from(entry).ok())
}

fn value_to_i32(value: Option<i64>) -> Option<i32> {
    value.and_then(|entry| i32::try_from(entry).ok())
}

fn extract_revision_tracker(content: &Value, captured_at: &str) -> Option<RevisionTrackerSnapshot> {
    let summary = content
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let changed_line_count = value_to_u32(content.get("changedLineCount").and_then(Value::as_u64));
    let first_changed_line = value_to_u32(content.get("firstChangedLine").and_then(Value::as_u64));
    let old_word_count = value_to_u32(content.get("oldWordCount").and_then(Value::as_u64));
    let new_word_count = value_to_u32(content.get("newWordCount").and_then(Value::as_u64));
    let delta_word_count = value_to_i32(content.get("deltaWordCount").and_then(Value::as_i64));

    if summary.is_none()
        && changed_line_count.is_none()
        && first_changed_line.is_none()
        && old_word_count.is_none()
        && new_word_count.is_none()
        && delta_word_count.is_none()
    {
        return None;
    }

    Some(RevisionTrackerSnapshot {
        changed_line_count,
        first_changed_line,
        old_word_count,
        new_word_count,
        delta_word_count,
        summary,
        updated_at: captured_at.to_string(),
    })
}

fn normalize_reference_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
}

fn normalize_relevance_tag(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|entry| !entry.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if matches!(normalized.as_str(), "high" | "medium" | "low") {
        Ok(Some(normalized))
    } else {
        Err("relevance_tag must be one of: high, medium, low.".to_string())
    }
}

fn reference_matches_selector(
    entry: &CollectedReference,
    doi: Option<&str>,
    pmid: Option<&str>,
    title: Option<&str>,
) -> bool {
    if let Some(doi) = doi {
        if entry
            .doi
            .as_deref()
            .map(|existing| existing.eq_ignore_ascii_case(doi))
            .unwrap_or(false)
        {
            return true;
        }
    }
    if let Some(pmid) = pmid {
        if entry.pmid.as_deref() == Some(pmid) {
            return true;
        }
    }
    if let Some(title) = title {
        return entry.title.eq_ignore_ascii_case(title);
    }
    false
}

fn same_reference(left: &CollectedReference, right: &CollectedReference) -> bool {
    if let (Some(left_doi), Some(right_doi)) = (left.doi.as_deref(), right.doi.as_deref()) {
        if left_doi.eq_ignore_ascii_case(right_doi) {
            return true;
        }
    }
    if let (Some(left_pmid), Some(right_pmid)) = (left.pmid.as_deref(), right.pmid.as_deref()) {
        if left_pmid == right_pmid {
            return true;
        }
    }
    let same_title = left.title.eq_ignore_ascii_case(&right.title);
    let same_year = left.year == right.year;
    same_title && (same_year || left.year.is_none() || right.year.is_none())
}

fn workflow_matches(
    workflow: &AgentWorkflowState,
    tab_id: &str,
    local_session_id: Option<&str>,
) -> bool {
    if workflow.tab_id == tab_id {
        return true;
    }
    if let Some(local_session_id) = local_session_id.filter(|value| !value.trim().is_empty()) {
        return workflow.local_session_id.as_deref() == Some(local_session_id);
    }
    false
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn pending_turn_with_ttl(mut pending: PendingTurnResume) -> PendingTurnResume {
    let created_at = if pending.created_at.trim().is_empty() {
        now_rfc3339()
    } else {
        pending.created_at.clone()
    };
    pending.created_at = created_at.clone();
    if pending.expires_at.trim().is_empty() {
        pending.expires_at = (parse_rfc3339_utc(&created_at).unwrap_or_else(Utc::now)
            + Duration::minutes(PENDING_TURN_TTL_MINUTES))
        .to_rfc3339();
    }
    pending
}

fn record_is_expired(record: &ToolApprovalRecord) -> bool {
    record
        .expires_at
        .as_deref()
        .and_then(parse_rfc3339_utc)
        .map(|expires_at| expires_at <= Utc::now())
        .unwrap_or(false)
}

fn pending_turn_is_expired(pending: &PendingTurnResume) -> bool {
    parse_rfc3339_utc(&pending.expires_at)
        .map(|expires_at| expires_at <= Utc::now())
        .unwrap_or(false)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn cleanup_expired_tool_approvals(
    approvals: &mut HashMap<String, HashMap<String, ToolApprovalRecord>>,
) -> bool {
    let before_entries = approvals
        .values()
        .map(|tool_map| tool_map.len())
        .sum::<usize>();
    approvals.retain(|_, tool_map| {
        tool_map.retain(|_, record| !record_is_expired(record));
        !tool_map.is_empty()
    });
    let after_entries = approvals
        .values()
        .map(|tool_map| tool_map.len())
        .sum::<usize>();
    before_entries != after_entries
}

fn cleanup_expired_pending_turns(pending_turns: &mut HashMap<String, PendingTurnResume>) -> bool {
    let before = pending_turns.len();
    pending_turns.retain(|_, pending| !pending_turn_is_expired(pending));
    before != pending_turns.len()
}

async fn load_memory_index(memory_dir: &PathBuf) -> MemoryIndex {
    let index_path = memory_dir.join(MEMORY_INDEX_FILE);
    match read_json_file::<MemoryIndex>(&index_path).await {
        Ok(index) => index,
        Err(_) => MemoryIndex::default(),
    }
}

fn estimate_memory_tokens(text: &str) -> usize {
    let mut cjk_chars = 0usize;
    let mut other_chars = 0usize;
    for c in text.chars() {
        if is_cjk_char(c) {
            cjk_chars += 1;
        } else {
            other_chars += 1;
        }
    }
    // CJK: ~1.5 tokens per character; ASCII: ~0.25 tokens per character
    (cjk_chars * 3 + other_chars).div_ceil(4)
}

fn is_cjk_char(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
}

async fn read_json_file<T>(path: &PathBuf) -> Result<T, String>
where
    T: Default + for<'de> Deserialize<'de>,
{
    match tokio::fs::read_to_string(path).await {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|err| format!("Failed to parse {}: {}", path.display(), err)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(err) => Err(format!("Failed to read {}: {}", path.display(), err)),
    }
}

async fn write_json_file<T>(path: &PathBuf, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let content = serde_json::to_string_pretty(value)
        .map_err(|err| format!("Failed to serialize {}: {}", path.display(), err))?;
    tokio::fs::write(path, content)
        .await
        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))
}

fn summarize_history_preview(items: &[AgentHistoryItem]) -> Option<String> {
    items
        .iter()
        .rev()
        .find_map(extract_history_text)
        .map(|text| truncate_preview(&text, 160))
}

fn extract_history_text(item: &AgentHistoryItem) -> Option<String> {
    let obj = item.as_object()?;
    let item_type = obj.get("type")?.as_str()?;
    if item_type != "assistant" && item_type != "user" && item_type != "result" {
        return None;
    }

    if let Some(result_text) = obj.get("result").and_then(|value| value.as_str()) {
        let trimmed = result_text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let content = obj
        .get("message")
        .and_then(|value| value.get("content"))
        .and_then(|value| value.as_array())?;

    for block in content {
        if let Some(text) = block.get("text").and_then(|value| value.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(thinking) = block.get("thinking").and_then(|value| value.as_str()) {
            let trimmed = thinking.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.replace('\n', " ");
    let trimmed = normalized.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    format!("{}...", trimmed.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::{AgentRuntimeState, AgentSessionRecord, PendingTurnResume};
    use crate::workflows::AgentWorkflowState;
    use serde_json::json;

    #[tokio::test]
    async fn session_summary_includes_working_memory_fields() {
        let state = AgentRuntimeState::default();
        let session = AgentSessionRecord::new(
            "minimax",
            "/tmp/project-a".to_string(),
            "tab-a".to_string(),
            "Draft Chat".to_string(),
            "MiniMax-M2.7".to_string(),
        );
        let local_session_id = session.local_session_id.clone();
        state
            .sessions
            .lock()
            .await
            .insert(local_session_id.clone(), session);

        state
            .set_current_objective(
                "tab-a",
                Some(&local_session_id),
                Some("Refine related work paragraph".to_string()),
            )
            .await;
        state
            .record_tool_running(
                "tab-a",
                Some(&local_session_id),
                "replace_selected_text",
                Some("main.tex"),
            )
            .await;
        state
            .mark_pending_state(
                "tab-a",
                Some(&local_session_id),
                "review_ready",
                "write_file",
                Some("main.tex"),
            )
            .await;

        let summary = state.session_summary(&local_session_id).await.unwrap();
        assert_eq!(
            summary.current_objective.as_deref(),
            Some("Refine related work paragraph")
        );
        assert_eq!(summary.current_target.as_deref(), Some("main.tex"));
        assert_eq!(summary.pending_state.as_deref(), Some("review_ready"));
        assert_eq!(summary.pending_target.as_deref(), Some("main.tex"));
        assert_eq!(
            summary.last_tool_activity.as_deref(),
            Some("Running replace_selected_text on main.tex")
        );
    }

    #[tokio::test]
    async fn set_current_objective_retains_previous_objective_for_recall() {
        let state = AgentRuntimeState::default();

        state
            .set_current_objective(
                "tab-a",
                None,
                Some("Refine related work paragraph".to_string()),
            )
            .await;
        state
            .set_current_objective(
                "tab-a",
                None,
                Some("Compare the attached papers for hydrophobic experiments.".to_string()),
            )
            .await;

        let work_state = state.work_state_for_prompt("tab-a", None).await;
        assert_eq!(
            work_state.current_objective.as_deref(),
            Some("Compare the attached papers for hydrophobic experiments.")
        );
        assert_eq!(
            work_state.recent_objective.as_deref(),
            Some("Refine related work paragraph")
        );
    }

    #[tokio::test]
    async fn literature_search_results_are_collected_into_session_memory() {
        let state = AgentRuntimeState::default();
        state
            .record_collected_references_from_tool_result(
                "tab-a",
                Some("session-a"),
                "search_literature",
                &json!({
                    "results": [
                        {
                            "title": "Hydrophobic Surface Engineering for Biomedical Implants",
                            "doi": "10.1000/example-doi",
                            "pmid": "12345678",
                            "year": 2024,
                            "journal": "Materials Advances",
                            "abstract": "Study on contact-angle-guided hydrophobic coatings.",
                            "authors": ["A. Author", "B. Author"]
                        }
                    ]
                }),
            )
            .await;

        let work_state = state
            .work_state_for_prompt("tab-a", Some("session-a"))
            .await;
        assert_eq!(work_state.collected_references.len(), 1);
        let first = &work_state.collected_references[0];
        assert_eq!(first.pmid.as_deref(), Some("12345678"));
        assert_eq!(first.doi.as_deref(), Some("10.1000/example-doi"));
    }

    #[tokio::test]
    async fn workflow_snapshot_syncs_into_session_work_state() {
        let state = AgentRuntimeState::default();
        let mut workflow =
            AgentWorkflowState::new_literature_review("tab-a", "/tmp/project-a", None);
        workflow.bind_local_session_id(Some("session-a"));
        workflow.current_stage = "paper_analysis".to_string();

        state.upsert_workflow_state(workflow).await;
        let work_state = state
            .work_state_for_prompt("tab-a", Some("session-a"))
            .await;
        assert_eq!(
            work_state.academic_workflow.workflow_type.as_deref(),
            Some("literature_review")
        );
        assert_eq!(
            work_state.academic_workflow.current_step.as_deref(),
            Some("paper_analysis")
        );
    }

    #[tokio::test]
    async fn review_and_revision_tool_results_are_persisted_in_work_state() {
        let state = AgentRuntimeState::default();
        state
            .record_academic_artifacts_from_tool_result(
                "tab-a",
                Some("session-a"),
                "review_manuscript",
                &json!({
                    "findings": [
                        {
                            "severity": "major",
                            "dimension": "scientific_rigor",
                            "message": "Objective is not clearly stated.",
                            "suggestion": "Add an objective paragraph in the introduction."
                        }
                    ]
                }),
            )
            .await;
        state
            .record_academic_artifacts_from_tool_result(
                "tab-a",
                Some("session-a"),
                "track_revisions",
                &json!({
                    "summary": "Tracked revisions: 5 changed lines, word delta 12 (210 -> 222).",
                    "changedLineCount": 5,
                    "firstChangedLine": 24,
                    "oldWordCount": 210,
                    "newWordCount": 222,
                    "deltaWordCount": 12
                }),
            )
            .await;

        let work_state = state
            .work_state_for_prompt("tab-a", Some("session-a"))
            .await;
        assert_eq!(work_state.academic_workflow.review_findings.len(), 1);
        assert_eq!(
            work_state.academic_workflow.review_findings[0].message,
            "Objective is not clearly stated."
        );
        let tracker = work_state
            .academic_workflow
            .revision_tracker
            .as_ref()
            .expect("revision tracker should be present");
        assert_eq!(tracker.changed_line_count, Some(5));
        assert_eq!(tracker.delta_word_count, Some(12));
    }

    #[tokio::test]
    async fn collected_reference_metadata_can_be_updated() {
        let state = AgentRuntimeState::default();
        state
            .record_collected_references_from_tool_result(
                "tab-a",
                Some("session-a"),
                "search_literature",
                &json!({
                    "results": [
                        {
                            "title": "Hydrophobic Surface Engineering for Biomedical Implants",
                            "doi": "10.1000/example-doi",
                            "pmid": "12345678",
                            "year": 2024
                        }
                    ]
                }),
            )
            .await;
        state
            .update_collected_reference(
                "tab-a",
                Some("session-a"),
                Some("10.1000/example-doi".to_string()),
                None,
                None,
                Some("Important for introduction related-work paragraph.".to_string()),
                Some("high".to_string()),
            )
            .await
            .expect("reference metadata should update");

        let refs = state
            .collected_references_for("tab-a", Some("session-a"))
            .await;
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].user_notes.as_deref(),
            Some("Important for introduction related-work paragraph.")
        );
        assert_eq!(refs[0].relevance_tag.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn pending_turn_round_trips_through_runtime_state() {
        let state = AgentRuntimeState::default();
        state
            .store_pending_turn(PendingTurnResume {
                project_path: "/tmp/project-a".to_string(),
                tab_id: "tab-a".to_string(),
                local_session_id: Some("session-a".to_string()),
                model: Some("MiniMax-M2.7".to_string()),
                turn_profile: None,
                approval_tool_name: "write_file".to_string(),
                target_label: Some("main.tex".to_string()),
                continuation_prompt: "Continue the suspended edit.".to_string(),
                created_at: String::new(),
                expires_at: String::new(),
            })
            .await;

        let pending = state.take_pending_turn("tab-a").await.unwrap();
        assert_eq!(pending.project_path, "/tmp/project-a");
        assert_eq!(pending.local_session_id.as_deref(), Some("session-a"));
        assert_eq!(pending.target_label.as_deref(), Some("main.tex"));
        assert!(state.take_pending_turn("tab-a").await.is_none());
    }

    #[tokio::test]
    async fn workflow_state_round_trips_through_runtime_state() {
        let state = AgentRuntimeState::default();
        let mut workflow = AgentWorkflowState::new_paper_drafting("tab-a", "/tmp/project-a", None);
        workflow.mark_stage_completed("Outline is done.");
        let workflow_id = workflow.workflow_id.clone();

        state.upsert_workflow_state(workflow.clone()).await;

        let loaded = state.workflow_state_for("tab-a", None).await.unwrap();
        assert_eq!(loaded.workflow_id, workflow_id);
        assert!(loaded.pending_checkpoint);
        assert_eq!(loaded.current_stage.as_str(), "outline_confirmation");

        state.clear_workflow_state("tab-a", None).await;
        assert!(state.workflow_state_for("tab-a", None).await.is_none());
    }
}
