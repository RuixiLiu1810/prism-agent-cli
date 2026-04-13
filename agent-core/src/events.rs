use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const AGENT_EVENT_NAME: &str = "agent-event";
pub const AGENT_COMPLETE_EVENT_NAME: &str = "agent-complete";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventEnvelope {
    pub tab_id: String,
    pub payload: AgentEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEventPayload {
    Status(AgentStatusEvent),
    MessageDelta(AgentMessageDeltaEvent),
    ToolCall(AgentToolCallEvent),
    ToolResult(AgentToolResultEvent),
    ToolInterrupt(AgentToolInterruptEvent),
    ApprovalRequested(AgentApprovalRequestedEvent),
    ReviewArtifactReady(AgentReviewArtifactReadyEvent),
    ToolResumed(AgentToolResumedEvent),
    TurnResumed(AgentTurnResumedEvent),
    WorkflowCheckpointRequested(AgentWorkflowCheckpointRequestedEvent),
    WorkflowCheckpointApproved(AgentWorkflowCheckpointApprovedEvent),
    WorkflowCheckpointRejected(AgentWorkflowCheckpointRejectedEvent),
    Error(AgentErrorEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusEvent {
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallEvent {
    pub tool_name: String,
    pub call_id: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResultEvent {
    pub tool_name: String,
    pub call_id: String,
    pub is_error: bool,
    pub preview: String,
    pub content: Value,
    pub display: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolInterruptPhase {
    AwaitingApproval,
    ReviewReady,
    Resumed,
    Cleared,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolInterruptEvent {
    pub phase: AgentToolInterruptPhase,
    pub tool_name: Option<String>,
    pub call_id: Option<String>,
    pub target_path: Option<String>,
    pub approval_tool_name: Option<String>,
    pub review_ready: bool,
    pub can_resume: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentApprovalRequestedEvent {
    pub tool_name: String,
    pub call_id: String,
    pub target_path: Option<String>,
    pub review_ready: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReviewArtifactReadyEvent {
    pub tool_name: String,
    pub call_id: String,
    pub target_path: String,
    pub summary: Option<String>,
    pub written: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResumedEvent {
    pub tool_name: String,
    pub target_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurnResumedEvent {
    pub local_session_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkflowCheckpointRequestedEvent {
    pub workflow_type: String,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkflowCheckpointApprovedEvent {
    pub workflow_type: String,
    pub from_stage: String,
    pub to_stage: String,
    pub completed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkflowCheckpointRejectedEvent {
    pub workflow_type: String,
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentErrorEvent {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCompletePayload {
    pub tab_id: String,
    pub outcome: String,
}
