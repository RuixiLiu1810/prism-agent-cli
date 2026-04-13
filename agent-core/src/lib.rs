// Agent core library — framework-agnostic agent runtime.

pub mod config;
pub mod events;
pub mod event_sink;
pub mod message_builder;
pub mod provider;
pub mod session;
pub mod streaming;
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
pub use streaming::{
    extract_function_call_item, extract_response_id, merge_stream_fragment, parse_sse_frame,
    push_reasoning_delta, sampling_profile_params, take_next_sse_frame,
    TOOL_ARGUMENTS_RETRY_HINT,
};
pub use message_builder::{
    effective_tool_choice_for_provider, extract_text_blocks_only, extract_text_segments,
    extract_tool_result_blocks, extract_tool_use_blocks, hidden_chat_message,
    provider_display_name, provider_supports_required_tool_choice, provider_supports_transport,
    raw_assistant_message, visible_assistant_message, visible_text_message,
    visible_tool_result_message,
};
