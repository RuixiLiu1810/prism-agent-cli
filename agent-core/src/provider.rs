use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub provider: String,
    pub display_name: String,
    pub ready: bool,
    pub mode: String,
    pub message: String,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskKind {
    #[default]
    General,
    SelectionEdit,
    FileEdit,
    SuggestionOnly,
    Analysis,
    LiteratureReview,
    PaperDrafting,
    PeerReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentSelectionScope {
    #[default]
    None,
    SelectedSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentResponseMode {
    #[default]
    Default,
    ReviewableChange,
    SuggestionOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentSamplingProfile {
    #[default]
    Default,
    EditStable,
    AnalysisBalanced,
    AnalysisDeep,
    ChatFlexible,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnProfile {
    #[serde(default)]
    pub task_kind: AgentTaskKind,
    #[serde(default)]
    pub selection_scope: AgentSelectionScope,
    #[serde(default)]
    pub response_mode: AgentResponseMode,
    #[serde(default)]
    pub sampling_profile: AgentSamplingProfile,
    #[serde(default)]
    pub source_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnDescriptor {
    pub project_path: String,
    pub prompt: String,
    pub tab_id: String,
    pub model: Option<String>,
    pub local_session_id: Option<String>,
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub turn_profile: Option<AgentTurnProfile>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnHandle {
    pub provider: String,
    pub local_session_id: String,
    pub response_id: Option<String>,
}

#[allow(dead_code)]
pub trait AgentProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn default_model(&self) -> Option<&'static str>;
    fn check_status(&self) -> AgentStatus;
    fn start_turn(&self, request: &AgentTurnDescriptor) -> Result<AgentTurnHandle, String>;
    fn continue_turn(&self, request: &AgentTurnDescriptor) -> Result<AgentTurnHandle, String>;
    fn cancel_turn(&self, response_id: &str) -> Result<(), String>;
}
