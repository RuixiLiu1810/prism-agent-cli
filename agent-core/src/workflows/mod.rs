mod coordinator;
mod literature_review;
mod paper_drafting;
mod peer_review;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

pub use literature_review::LiteratureReviewStage;
pub use paper_drafting::PaperDraftingStage;
pub use peer_review::PeerReviewStage;
pub use coordinator::run_subagent_turn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkflowType {
    PaperDrafting,
    LiteratureReview,
    PeerReview,
}

impl AgentWorkflowType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaperDrafting => "paper_drafting",
            Self::LiteratureReview => "literature_review",
            Self::PeerReview => "peer_review",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCheckpointDecision {
    ApproveStage,
    RequestChanges,
}

impl WorkflowCheckpointDecision {
    #[expect(
        clippy::should_implement_trait,
        reason = "Keep the associated parsing entrypoint stable for desktop call sites outside this write scope"
    )]
    pub fn from_str(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

impl FromStr for WorkflowCheckpointDecision {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "approve" | "approve_stage" => Ok(Self::ApproveStage),
            "reject" | "request_changes" | "request_change" => Ok(Self::RequestChanges),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStageRecord {
    pub stage: String,
    pub prompt_summary: Option<String>,
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkflowState {
    pub workflow_id: String,
    pub workflow_type: AgentWorkflowType,
    pub tab_id: String,
    pub local_session_id: Option<String>,
    pub project_path: String,
    pub model: Option<String>,
    pub current_stage: String,
    pub pending_checkpoint: bool,
    pub stage_history: Vec<WorkflowStageRecord>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointTransition {
    pub workflow_type: AgentWorkflowType,
    pub from_stage: String,
    pub to_stage: String,
    pub completed: bool,
}

impl AgentWorkflowState {
    pub fn new_paper_drafting(tab_id: &str, project_path: &str, model: Option<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            workflow_type: AgentWorkflowType::PaperDrafting,
            tab_id: tab_id.to_string(),
            local_session_id: None,
            project_path: project_path.to_string(),
            model,
            current_stage: PaperDraftingStage::OutlineConfirmation.as_str().to_string(),
            pending_checkpoint: false,
            stage_history: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn new_literature_review(tab_id: &str, project_path: &str, model: Option<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            workflow_type: AgentWorkflowType::LiteratureReview,
            tab_id: tab_id.to_string(),
            local_session_id: None,
            project_path: project_path.to_string(),
            model,
            current_stage: LiteratureReviewStage::PicoScoping.as_str().to_string(),
            pending_checkpoint: false,
            stage_history: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn new_peer_review(tab_id: &str, project_path: &str, model: Option<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            workflow_id: Uuid::new_v4().to_string(),
            workflow_type: AgentWorkflowType::PeerReview,
            tab_id: tab_id.to_string(),
            local_session_id: None,
            project_path: project_path.to_string(),
            model,
            current_stage: PeerReviewStage::ScopeAndCriteria.as_str().to_string(),
            pending_checkpoint: false,
            stage_history: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn stage_label(&self) -> String {
        match self.workflow_type {
            AgentWorkflowType::PaperDrafting => parse_paper_stage(&self.current_stage)
                .map(|stage| stage.label().to_string())
                .unwrap_or_else(|| self.current_stage.clone()),
            AgentWorkflowType::LiteratureReview => parse_literature_stage(&self.current_stage)
                .map(|stage| stage.label().to_string())
                .unwrap_or_else(|| self.current_stage.clone()),
            AgentWorkflowType::PeerReview => parse_peer_review_stage(&self.current_stage)
                .map(|stage| stage.label().to_string())
                .unwrap_or_else(|| self.current_stage.clone()),
        }
    }

    pub fn is_completed(&self) -> bool {
        match self.workflow_type {
            AgentWorkflowType::PaperDrafting => parse_paper_stage(&self.current_stage)
                .map(|stage| stage.is_terminal())
                .unwrap_or(false),
            AgentWorkflowType::LiteratureReview => parse_literature_stage(&self.current_stage)
                .map(|stage| stage.is_terminal())
                .unwrap_or(false),
            AgentWorkflowType::PeerReview => parse_peer_review_stage(&self.current_stage)
                .map(|stage| stage.is_terminal())
                .unwrap_or(false),
        }
    }

    pub fn can_run_stage(&self) -> Result<(), String> {
        if self.pending_checkpoint {
            return Err(format!(
                "Workflow checkpoint is pending at stage '{}'. Approve or request changes before continuing.",
                self.current_stage
            ));
        }
        if self.is_completed() {
            return Err(
                "Workflow already completed. Start a new workflow to continue.".to_string(),
            );
        }
        Ok(())
    }

    pub fn mark_stage_completed(&mut self, prompt: &str) {
        self.pending_checkpoint = true;
        self.stage_history.push(WorkflowStageRecord {
            stage: self.current_stage.clone(),
            prompt_summary: summarize_prompt(prompt),
            completed_at: Utc::now().to_rfc3339(),
        });
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn apply_checkpoint_decision(
        &mut self,
        decision: WorkflowCheckpointDecision,
    ) -> Result<WorkflowCheckpointTransition, String> {
        if !self.pending_checkpoint {
            return Err("No pending workflow checkpoint to resolve.".to_string());
        }

        let from = self.current_stage.clone();
        match decision {
            WorkflowCheckpointDecision::ApproveStage => {
                self.pending_checkpoint = false;
                if let Some(next_stage) = self.next_stage() {
                    self.current_stage = next_stage;
                } else {
                    self.current_stage = "completed".to_string();
                }
                self.updated_at = Utc::now().to_rfc3339();
                Ok(WorkflowCheckpointTransition {
                    workflow_type: self.workflow_type.clone(),
                    from_stage: from,
                    to_stage: self.current_stage.clone(),
                    completed: self.is_completed(),
                })
            }
            WorkflowCheckpointDecision::RequestChanges => {
                self.pending_checkpoint = false;
                self.updated_at = Utc::now().to_rfc3339();
                Ok(WorkflowCheckpointTransition {
                    workflow_type: self.workflow_type.clone(),
                    from_stage: from.clone(),
                    to_stage: from,
                    completed: false,
                })
            }
        }
    }

    pub fn bind_local_session_id(&mut self, local_session_id: Option<&str>) {
        if let Some(local_session_id) = local_session_id.filter(|value| !value.trim().is_empty()) {
            self.local_session_id = Some(local_session_id.to_string());
            self.updated_at = Utc::now().to_rfc3339();
        }
    }

    pub fn build_stage_prompt(&self, user_prompt: &str) -> String {
        let mut lines = vec![
            format!("[Workflow mode: {}]", self.workflow_type.as_str()),
            format!("[Workflow stage: {}]", self.current_stage),
            format!("[Stage objective: {}]", self.stage_instruction()),
            "[Checkpoint rule: complete only this stage in this turn; do not silently advance to the next stage.]".to_string(),
        ];

        if !self.stage_history.is_empty() {
            lines.push("[Completed stages so far:]".to_string());
            for entry in self.stage_history.iter().rev().take(3).rev() {
                let suffix = entry
                    .prompt_summary
                    .as_deref()
                    .map(|summary| format!(" — {}", summary))
                    .unwrap_or_default();
                lines.push(format!("- {}{}", entry.stage, suffix));
            }
        }

        format!("{}\n\n{}", lines.join("\n"), user_prompt)
    }

    fn next_stage(&self) -> Option<String> {
        match self.workflow_type {
            AgentWorkflowType::PaperDrafting => parse_paper_stage(&self.current_stage)
                .and_then(|stage| stage.next_stage())
                .map(|stage| stage.as_str().to_string()),
            AgentWorkflowType::LiteratureReview => parse_literature_stage(&self.current_stage)
                .and_then(|stage| stage.next_stage())
                .map(|stage| stage.as_str().to_string()),
            AgentWorkflowType::PeerReview => parse_peer_review_stage(&self.current_stage)
                .and_then(|stage| stage.next_stage())
                .map(|stage| stage.as_str().to_string()),
        }
    }

    fn stage_instruction(&self) -> String {
        match self.workflow_type {
            AgentWorkflowType::PaperDrafting => parse_paper_stage(&self.current_stage)
                .map(|stage| stage.instruction().to_string())
                .unwrap_or_else(|| "Complete the current workflow stage.".to_string()),
            AgentWorkflowType::LiteratureReview => parse_literature_stage(&self.current_stage)
                .map(|stage| stage.instruction().to_string())
                .unwrap_or_else(|| "Complete the current workflow stage.".to_string()),
            AgentWorkflowType::PeerReview => parse_peer_review_stage(&self.current_stage)
                .map(|stage| stage.instruction().to_string())
                .unwrap_or_else(|| "Complete the current workflow stage.".to_string()),
        }
    }
}

fn parse_paper_stage(value: &str) -> Option<PaperDraftingStage> {
    match value.trim().to_ascii_lowercase().as_str() {
        "outline_confirmation" => Some(PaperDraftingStage::OutlineConfirmation),
        "section_drafting" => Some(PaperDraftingStage::SectionDrafting),
        "consistency_check" => Some(PaperDraftingStage::ConsistencyCheck),
        "revision_pass" => Some(PaperDraftingStage::RevisionPass),
        "final_packaging" => Some(PaperDraftingStage::FinalPackaging),
        "completed" => Some(PaperDraftingStage::Completed),
        _ => None,
    }
}

fn parse_literature_stage(value: &str) -> Option<LiteratureReviewStage> {
    match value.trim().to_ascii_lowercase().as_str() {
        "pico_scoping" => Some(LiteratureReviewStage::PicoScoping),
        "search_and_screen" => Some(LiteratureReviewStage::SearchAndScreen),
        "paper_analysis" => Some(LiteratureReviewStage::PaperAnalysis),
        "evidence_synthesis" => Some(LiteratureReviewStage::EvidenceSynthesis),
        "completed" => Some(LiteratureReviewStage::Completed),
        _ => None,
    }
}

fn parse_peer_review_stage(value: &str) -> Option<PeerReviewStage> {
    match value.trim().to_ascii_lowercase().as_str() {
        "scope_and_criteria" => Some(PeerReviewStage::ScopeAndCriteria),
        "section_review" => Some(PeerReviewStage::SectionReview),
        "statistics_review" => Some(PeerReviewStage::StatisticsReview),
        "report_and_revision_plan" => Some(PeerReviewStage::ReportAndRevisionPlan),
        "completed" => Some(PeerReviewStage::Completed),
        _ => None,
    }
}

fn summarize_prompt(prompt: &str) -> Option<String> {
    prompt
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('['))
        .map(|line| {
            if line.chars().count() > 160 {
                format!("{}...", line.chars().take(160).collect::<String>())
            } else {
                line.to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{AgentWorkflowState, WorkflowCheckpointDecision};

    #[test]
    fn paper_drafting_workflow_advances_only_after_checkpoint_approval() {
        let mut workflow = AgentWorkflowState::new_paper_drafting("tab-1", "/tmp/project", None);

        assert!(workflow.can_run_stage().is_ok());
        workflow.mark_stage_completed("Drafted the outline.");
        assert!(workflow.can_run_stage().is_err());

        let transition = workflow
            .apply_checkpoint_decision(WorkflowCheckpointDecision::ApproveStage)
            .unwrap();
        assert_eq!(transition.from_stage, "outline_confirmation");
        assert_eq!(transition.to_stage, "section_drafting");
        assert!(!transition.completed);
        assert!(workflow.can_run_stage().is_ok());
    }

    #[test]
    fn literature_review_workflow_advances_after_approval() {
        let mut workflow = AgentWorkflowState::new_literature_review("tab-1", "/tmp/project", None);
        assert_eq!(workflow.current_stage, "pico_scoping");
        workflow.mark_stage_completed("Defined PICO.");

        let transition = workflow
            .apply_checkpoint_decision(WorkflowCheckpointDecision::ApproveStage)
            .unwrap();
        assert_eq!(transition.from_stage, "pico_scoping");
        assert_eq!(transition.to_stage, "search_and_screen");
        assert!(!transition.completed);
    }

    #[test]
    fn peer_review_workflow_advances_after_approval() {
        let mut workflow = AgentWorkflowState::new_peer_review("tab-1", "/tmp/project", None);
        assert_eq!(workflow.current_stage, "scope_and_criteria");
        workflow.mark_stage_completed("Defined review scope and criteria.");

        let transition = workflow
            .apply_checkpoint_decision(WorkflowCheckpointDecision::ApproveStage)
            .unwrap();
        assert_eq!(transition.from_stage, "scope_and_criteria");
        assert_eq!(transition.to_stage, "section_review");
        assert!(!transition.completed);
    }
}
