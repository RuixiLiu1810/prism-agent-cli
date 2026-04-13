use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::provider::AgentTaskKind;

const MAX_PREVIEW_CHARS: usize = 240;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCapabilityClass {
    ReadTextFile,
    ReadDocument,
    SearchDocument,
    InspectResource,
    LiteratureAnalysis,
    DraftWriting,
    ReviewWriting,
    EditPatch,
    EditWrite,
    ListWorkspace,
    SearchWorkspace,
    ExecuteShell,
    MemoryWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResourceScope {
    TextFile,
    Document,
    Workspace,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalPolicy {
    Never,
    SessionScoped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolReviewPolicy {
    None,
    DiffRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSuspendBehavior {
    None,
    SuspendOnApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultShape {
    TextRead,
    DocumentExcerpt,
    DocumentSearch,
    ResourceInfo,
    LiteratureOutput,
    WritingOutput,
    WorkspaceSearch,
    ReviewArtifact,
    CommandOutput,
}

// ---------------------------------------------------------------------------
// AgentToolContract
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentToolContract {
    pub capability_class: ToolCapabilityClass,
    pub resource_scope: ToolResourceScope,
    pub approval_policy: ToolApprovalPolicy,
    pub review_policy: ToolReviewPolicy,
    pub suspend_behavior: ToolSuspendBehavior,
    pub result_shape: ToolResultShape,
    pub parallel_safe: bool,
    pub approval_bucket: &'static str,
}

// ---------------------------------------------------------------------------
// tool_contract
// ---------------------------------------------------------------------------

pub fn tool_contract(tool_name: &str) -> AgentToolContract {
    match tool_name {
        "read_file" => AgentToolContract {
            capability_class: ToolCapabilityClass::ReadTextFile,
            resource_scope: ToolResourceScope::TextFile,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::TextRead,
            parallel_safe: true,
            approval_bucket: "read_file",
        },
        "read_document" => AgentToolContract {
            capability_class: ToolCapabilityClass::ReadDocument,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::DocumentExcerpt,
            parallel_safe: true,
            approval_bucket: "read_document",
        },
        "search_literature" | "analyze_paper" | "compare_papers" | "synthesize_evidence" | "extract_methodology" => AgentToolContract {
            capability_class: ToolCapabilityClass::LiteratureAnalysis,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::LiteratureOutput,
            parallel_safe: true,
            approval_bucket: "literature_analysis",
        },
        "inspect_resource" => AgentToolContract {
            capability_class: ToolCapabilityClass::InspectResource,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::ResourceInfo,
            parallel_safe: true,
            approval_bucket: "inspect_resource",
        },
        "read_document_excerpt" => AgentToolContract {
            capability_class: ToolCapabilityClass::ReadDocument,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::DocumentExcerpt,
            parallel_safe: true,
            approval_bucket: "read_document_excerpt",
        },
        "search_document_text" => AgentToolContract {
            capability_class: ToolCapabilityClass::SearchDocument,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::DocumentSearch,
            parallel_safe: true,
            approval_bucket: "search_document_text",
        },
        "get_document_evidence" => AgentToolContract {
            capability_class: ToolCapabilityClass::SearchDocument,
            resource_scope: ToolResourceScope::Document,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::DocumentSearch,
            parallel_safe: true,
            approval_bucket: "get_document_evidence",
        },
        "draft_section" | "restructure_outline" | "generate_abstract" => AgentToolContract {
            capability_class: ToolCapabilityClass::DraftWriting,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WritingOutput,
            parallel_safe: true,
            approval_bucket: "writing_draft",
        },
        "check_consistency" | "insert_citation" => AgentToolContract {
            capability_class: ToolCapabilityClass::ReviewWriting,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WritingOutput,
            parallel_safe: true,
            approval_bucket: "writing_review",
        },
        "review_manuscript" | "check_statistics" | "verify_references" | "generate_response_letter" | "track_revisions" => AgentToolContract {
            capability_class: ToolCapabilityClass::ReviewWriting,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WritingOutput,
            parallel_safe: true,
            approval_bucket: "peer_review",
        },
        "replace_selected_text" | "apply_text_patch" => AgentToolContract {
            capability_class: ToolCapabilityClass::EditPatch,
            resource_scope: ToolResourceScope::TextFile,
            approval_policy: ToolApprovalPolicy::SessionScoped,
            review_policy: ToolReviewPolicy::DiffRequired,
            suspend_behavior: ToolSuspendBehavior::SuspendOnApproval,
            result_shape: ToolResultShape::ReviewArtifact,
            parallel_safe: false,
            approval_bucket: "patch_file",
        },
        "write_file" => AgentToolContract {
            capability_class: ToolCapabilityClass::EditWrite,
            resource_scope: ToolResourceScope::TextFile,
            approval_policy: ToolApprovalPolicy::SessionScoped,
            review_policy: ToolReviewPolicy::DiffRequired,
            suspend_behavior: ToolSuspendBehavior::SuspendOnApproval,
            result_shape: ToolResultShape::ReviewArtifact,
            parallel_safe: false,
            approval_bucket: "write_file",
        },
        "list_files" => AgentToolContract {
            capability_class: ToolCapabilityClass::ListWorkspace,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WorkspaceSearch,
            parallel_safe: true,
            approval_bucket: "list_files",
        },
        "search_project" => AgentToolContract {
            capability_class: ToolCapabilityClass::SearchWorkspace,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WorkspaceSearch,
            parallel_safe: true,
            approval_bucket: "search_project",
        },
        "run_shell_command" => AgentToolContract {
            capability_class: ToolCapabilityClass::ExecuteShell,
            resource_scope: ToolResourceScope::Shell,
            approval_policy: ToolApprovalPolicy::SessionScoped,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::SuspendOnApproval,
            result_shape: ToolResultShape::CommandOutput,
            parallel_safe: false,
            approval_bucket: "run_shell_command",
        },
        "remember_fact" => AgentToolContract {
            capability_class: ToolCapabilityClass::MemoryWrite,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::TextRead,
            parallel_safe: false,
            approval_bucket: "remember_fact",
        },
        _ => AgentToolContract {
            capability_class: ToolCapabilityClass::SearchWorkspace,
            resource_scope: ToolResourceScope::Workspace,
            approval_policy: ToolApprovalPolicy::Never,
            review_policy: ToolReviewPolicy::None,
            suspend_behavior: ToolSuspendBehavior::None,
            result_shape: ToolResultShape::WorkspaceSearch,
            parallel_safe: false,
            approval_bucket: "unknown",
        },
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

pub fn is_reviewable_edit_tool(tool_name: &str) -> bool {
    tool_contract(tool_name).review_policy == ToolReviewPolicy::DiffRequired
}

pub fn is_parallel_safe_tool(tool_name: &str) -> bool {
    tool_contract(tool_name).parallel_safe
}

pub fn is_document_tool_name(tool_name: &str) -> bool {
    matches!(tool_contract(tool_name).resource_scope, ToolResourceScope::Document)
}

pub fn tool_display_kind(tool_name: &str) -> &'static str {
    match tool_contract(tool_name).capability_class {
        ToolCapabilityClass::ReadTextFile => "text_read",
        ToolCapabilityClass::ReadDocument => "document_read",
        ToolCapabilityClass::SearchDocument => "document_search",
        ToolCapabilityClass::InspectResource => "resource_info",
        ToolCapabilityClass::LiteratureAnalysis => "literature_analysis",
        ToolCapabilityClass::DraftWriting => "writing_draft",
        ToolCapabilityClass::ReviewWriting => "writing_review",
        ToolCapabilityClass::EditPatch => "edit_patch",
        ToolCapabilityClass::EditWrite => "edit_write",
        ToolCapabilityClass::ListWorkspace | ToolCapabilityClass::SearchWorkspace => "workspace_search",
        ToolCapabilityClass::ExecuteShell => "shell_command",
        ToolCapabilityClass::MemoryWrite => "memory_write",
    }
}

pub fn approval_bucket_for_tool(tool_name: &str) -> &str {
    tool_contract(tool_name).approval_bucket
}

// ---------------------------------------------------------------------------
// Document resource path helpers
// ---------------------------------------------------------------------------

pub fn resource_kind_from_path(path: &str) -> &'static str {
    let lower = path.trim().to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        "pdf_document"
    } else if lower.ends_with(".docx") {
        "docx_document"
    } else {
        "text_file"
    }
}

pub fn is_document_resource_path(path: &str) -> bool {
    matches!(resource_kind_from_path(path), "pdf_document" | "docx_document")
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub contract: AgentToolContract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCall {
    pub tool_name: String,
    pub call_id: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResult {
    pub tool_name: String,
    pub call_id: String,
    pub is_error: bool,
    pub content: Value,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResultDisplayContent {
    pub kind: String,
    pub display_kind: String,
    pub tool_name: String,
    pub status: String,
    pub text_preview: String,
    pub is_error: bool,
    pub target_path: Option<String>,
    pub command: Option<String>,
    pub query: Option<String>,
    pub approval_required: bool,
    pub review_ready: bool,
    pub approval_reason: Option<String>,
    pub approval_tool_name: Option<String>,
    pub written: Option<bool>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionPolicyContext {
    pub task_kind: AgentTaskKind,
    pub has_binary_attachment_context: bool,
}

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

pub fn truncate_preview(value: &str) -> String {
    if value.chars().count() > MAX_PREVIEW_CHARS {
        format!("{}...", value.chars().take(MAX_PREVIEW_CHARS).collect::<String>())
    } else {
        value.to_string()
    }
}

pub fn error_result(tool_name: &str, call_id: &str, message: String) -> AgentToolResult {
    AgentToolResult {
        tool_name: tool_name.to_string(),
        call_id: call_id.to_string(),
        is_error: true,
        preview: truncate_preview(&message),
        content: json!({ "error": message }),
    }
}

pub fn summarize_tool_target(content: &Value) -> Option<String> {
    ["path", "file_path", "targetPath", "filePath", "command", "query"]
        .iter()
        .find_map(|key| content.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn tool_result_requires_approval(result: &AgentToolResult) -> bool {
    result.content.get("approvalRequired").and_then(Value::as_bool).unwrap_or(false)
}

pub fn tool_result_review_ready(result: &AgentToolResult) -> bool {
    result.content.get("reviewArtifact").and_then(Value::as_bool).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

pub fn tool_result_display_content(result: &AgentToolResult) -> AgentToolResultDisplayContent {
    let review_artifact_payload = result.content.get("reviewArtifactPayload").and_then(Value::as_object);
    let target_path = summarize_tool_target(&result.content).or_else(|| {
        review_artifact_payload.and_then(|payload| {
            payload.get("targetPath").and_then(Value::as_str).map(str::to_string)
        })
    });
    let command = result.content.get("command").and_then(Value::as_str).map(str::to_string).or_else(|| {
        result.content.get("input").and_then(Value::as_object).and_then(|payload| payload.get("command")).and_then(Value::as_str).map(str::to_string)
    });
    let query = result.content.get("query").and_then(Value::as_str).map(str::to_string).or_else(|| {
        result.content.get("input").and_then(Value::as_object).and_then(|payload| payload.get("query")).and_then(Value::as_str).map(str::to_string)
    });
    let approval_required = tool_result_requires_approval(result);
    let review_ready = tool_result_review_ready(result);
    let approval_reason = result.content.get("reason").and_then(Value::as_str).map(str::to_string);
    let approval_tool_name = result.content.get("approvalToolName").and_then(Value::as_str)
        .or_else(|| result.content.get("toolName").and_then(Value::as_str)).map(str::to_string);
    let summary = review_artifact_payload
        .and_then(|payload| payload.get("summary"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| result.content.get("summary").and_then(Value::as_str).map(str::to_string));
    let content_text = result.content.get("content").and_then(Value::as_str).map(str::to_string);
    let error_text = result.content.get("error").and_then(Value::as_str).map(str::to_string);
    let status = if approval_required && review_ready {
        "review_ready"
    } else if approval_required {
        "awaiting_approval"
    } else if result.is_error {
        "error"
    } else {
        "completed"
    };

    AgentToolResultDisplayContent {
        kind: "tool_result_display".to_string(),
        display_kind: tool_display_kind(&result.tool_name).to_string(),
        tool_name: result.tool_name.clone(),
        status: status.to_string(),
        text_preview: error_text.or(content_text).or(approval_reason.clone()).or(summary.clone()).unwrap_or_else(|| result.preview.clone()),
        is_error: result.is_error,
        target_path,
        command,
        query,
        approval_required,
        review_ready,
        approval_reason,
        approval_tool_name,
        written: result.content.get("written").and_then(Value::as_bool),
        summary,
    }
}

pub fn tool_result_display_value(result: &AgentToolResult) -> Value {
    serde_json::to_value(tool_result_display_content(result)).unwrap_or_else(|_| {
        json!({
            "kind": "tool_result_display",
            "displayKind": tool_display_kind(&result.tool_name),
            "toolName": result.tool_name,
            "status": if result.is_error { "error" } else { "completed" },
            "textPreview": result.preview,
            "isError": result.is_error,
            "approvalRequired": tool_result_requires_approval(result),
            "reviewReady": tool_result_review_ready(result),
        })
    })
}

// ---------------------------------------------------------------------------
// Policy enforcement
// ---------------------------------------------------------------------------

pub fn check_tool_call_policy(
    context: ToolExecutionPolicyContext,
    call: &AgentToolCall,
    target: Option<&str>,
) -> Option<AgentToolResult> {
    if context.task_kind == AgentTaskKind::SelectionEdit && call.tool_name == "write_file" {
        return Some(AgentToolResult {
            tool_name: call.tool_name.clone(),
            call_id: call.call_id.clone(),
            is_error: true,
            preview: "Selection-scoped edits must not use write_file.".to_string(),
            content: json!({
                "error": "selection-scoped edits must not use write_file",
                "disallowedByPolicy": true,
                "attemptedTool": "write_file",
                "suggestedTools": ["replace_selected_text", "apply_text_patch"]
            }),
        });
    }

    if matches!(context.task_kind, AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview)
        && context.has_binary_attachment_context
        && call.tool_name == "run_shell_command"
    {
        return Some(AgentToolResult {
            tool_name: call.tool_name.clone(),
            call_id: call.call_id.clone(),
            is_error: true,
            preview: "Attachment-backed PDF/DOCX analysis must not use shell probing.".to_string(),
            content: json!({
                "error": "attachment-backed PDF/DOCX analysis must not use run_shell_command for exploratory extraction",
                "disallowedByPolicy": true,
                "attemptedTool": "run_shell_command",
                "suggestedAction": "Use read_document instead."
            }),
        });
    }

    if matches!(context.task_kind, AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview)
        && context.has_binary_attachment_context
        && call.tool_name == "read_file"
        && target.map(is_document_resource_path).unwrap_or(false)
    {
        return Some(AgentToolResult {
            tool_name: call.tool_name.clone(),
            call_id: call.call_id.clone(),
            is_error: true,
            preview: "Binary attachment analysis must rely on ingested excerpts, not raw file reads.".to_string(),
            content: json!({
                "error": "attachment-backed PDF/DOCX analysis must not use read_file on binary resources",
                "disallowedByPolicy": true,
                "attemptedTool": "read_file",
                "suggestedAction": "Use read_document or search_document_text instead."
            }),
        });
    }

    None
}
