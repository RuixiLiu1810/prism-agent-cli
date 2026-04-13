// Agent core library — framework-agnostic agent runtime.

pub mod config;
pub mod events;
pub mod event_sink;
pub mod provider;
pub mod session;
pub mod tools;
pub mod turn_engine;
pub mod workflows;

pub use config::{
    AgentDomainConfig, AgentRuntimeConfig, AgentSamplingConfig, AgentSamplingProfilesConfig,
    ConfigProvider, StaticConfigProvider,
};
pub use event_sink::{EventSink, NullEventSink};
pub use events::*;
pub use provider::{
    AgentProvider, AgentResponseMode, AgentSamplingProfile, AgentSelectionScope, AgentStatus,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnHandle, AgentTurnProfile,
};
pub use tools::{
    AgentToolCall, AgentToolContract, AgentToolResult, AgentToolResultDisplayContent,
    AgentToolSpec, ToolApprovalPolicy, ToolCapabilityClass, ToolExecutionPolicyContext,
    ToolResourceScope, ToolResultShape, ToolReviewPolicy, ToolSuspendBehavior,
};
pub use turn_engine::{
    compact_chat_messages, emit_agent_complete, emit_approval_requested, emit_error, emit_status,
    emit_text_delta, emit_tool_call, emit_tool_interrupt_state, emit_tool_result,
    emit_tool_resumed, emit_turn_resumed, emit_workflow_checkpoint_approved,
    emit_workflow_checkpoint_rejected, emit_workflow_checkpoint_requested, estimate_tokens,
    request_has_binary_attachment_context, should_surface_assistant_text,
    tool_result_feedback_for_model, tool_result_has_invalid_arguments_error, tool_result_status,
    ExecutedToolBatch, ExecutedToolCall, ToolCallTracker, TurnBudget, AGENT_CANCELLED_MESSAGE,
};
pub use workflows::{
    AgentWorkflowState, AgentWorkflowType, WorkflowCheckpointDecision,
    WorkflowCheckpointTransition, WorkflowStageRecord,
};
pub use session::{
    AgentRuntimeState, AgentSessionRecord, AgentSessionSummary, AgentSessionWorkState,
    CollectedReference, MemoryEntry, MemoryIndex, MemoryType, PendingTurnResume,
    ToolApprovalDecision, ToolApprovalRecord, ToolApprovalState,
};
