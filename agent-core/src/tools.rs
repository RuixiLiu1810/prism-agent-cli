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
        "search_literature"
        | "analyze_paper"
        | "compare_papers"
        | "synthesize_evidence"
        | "extract_methodology" => AgentToolContract {
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
        "review_manuscript"
        | "check_statistics"
        | "verify_references"
        | "generate_response_letter"
        | "track_revisions" => AgentToolContract {
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
    matches!(
        tool_contract(tool_name).resource_scope,
        ToolResourceScope::Document
    )
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
        ToolCapabilityClass::ListWorkspace | ToolCapabilityClass::SearchWorkspace => {
            "workspace_search"
        }
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
    matches!(
        resource_kind_from_path(path),
        "pdf_document" | "docx_document"
    )
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

fn make_tool_spec(name: &str, description: &str, input_schema: Value) -> AgentToolSpec {
    AgentToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        contract: tool_contract(name),
    }
}

fn writing_tools_enabled() -> bool {
    std::env::var("PRISM_AGENT_WRITING_TOOLS")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized != "0" && normalized != "false" && normalized != "off"
        })
        .unwrap_or(true)
}

pub fn default_tool_specs() -> Vec<AgentToolSpec> {
    build_default_tool_specs(writing_tools_enabled())
}

fn build_default_tool_specs(include_writing_tools: bool) -> Vec<AgentToolSpec> {
    let mut specs = vec![
        make_tool_spec(
            "read_file",
            "Read a text file from the current project. Supported: source code, markdown, JSON, CSV, and plain text. Do not use this tool for PDF, DOCX, images, or other binary resources.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative file path." }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "read_document",
            "Read an attached or project document resource (PDF/DOCX) using the runtime ingestion pipeline. Provide path always, and optionally query to extract targeted evidence snippets.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative path for the document resource." },
                    "query": { "type": "string", "description": "Optional question or search query for targeted evidence." },
                    "limit": { "type": "integer", "description": "Optional maximum number of evidence snippets to return when query is provided." }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "search_literature",
            "Search academic literature across multiple providers (Semantic Scholar, OpenAlex, Crossref, PubMed). Supports optional MeSH expansion and year filters. Returns structured citation candidates.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Literature query describing the research topic, claim, or question." },
                    "limit": { "type": "integer", "description": "Optional maximum number of merged results (default 10)." },
                    "mesh_expansion": { "type": "boolean", "description": "Whether to include MeSH-oriented query expansions (default true)." },
                    "min_year": { "type": "integer", "description": "Optional lower publication year bound." },
                    "max_year": { "type": "integer", "description": "Optional upper publication year bound." }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "analyze_paper",
            "Analyze an ingested PDF/DOCX paper and return objective, methods, findings, limitations, and relevance.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative path to PDF/DOCX resource." },
                    "focus": { "type": "string", "description": "Optional focus question to score relevance." },
                    "max_items": { "type": "integer", "description": "Optional max number of extracted method/finding/limitation items." }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "compare_papers",
            "Compare multiple papers and return shared findings, conflicting signals, and methodology differences.",
            json!({
                "type": "object",
                "properties": {
                    "papers": {
                        "type": "array",
                        "description": "Array of paper paths or objects with path field.",
                        "items": {
                            "oneOf": [
                                { "type": "string" },
                                {
                                    "type": "object",
                                    "properties": {
                                        "path": { "type": "string" }
                                    },
                                    "required": ["path"],
                                    "additionalProperties": false
                                }
                            ]
                        }
                    },
                    "focus": { "type": "string", "description": "Optional comparison focus question." }
                },
                "required": ["papers"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "synthesize_evidence",
            "Synthesize evidence from multiple papers into theme-organized blocks with source-linked support.",
            json!({
                "type": "object",
                "properties": {
                    "papers": {
                        "type": "array",
                        "description": "Array of paper paths or objects with path field.",
                        "items": {
                            "oneOf": [
                                { "type": "string" },
                                {
                                    "type": "object",
                                    "properties": {
                                        "path": { "type": "string" }
                                    },
                                    "required": ["path"],
                                    "additionalProperties": false
                                }
                            ]
                        }
                    },
                    "focus": { "type": "string", "description": "Optional synthesis focus question." }
                },
                "required": ["papers"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "extract_methodology",
            "Extract structured methodology fields from a paper: study design, sample, intervention, endpoints, and statistics.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative path to PDF/DOCX resource." }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
    ];

    if include_writing_tools {
        specs.extend([
            make_tool_spec(
                "draft_section",
                "Draft a manuscript section from structured key points. Use this for first-pass writing in English academic style, then refine with follow-up edits.",
                json!({
                    "type": "object",
                    "properties": {
                        "section_type": { "type": "string", "description": "Section type such as introduction, methods, results, discussion, or conclusion." },
                        "key_points": {
                            "description": "Core points to include. Pass either an array of strings or a newline-delimited string.",
                            "oneOf": [
                                { "type": "array", "items": { "type": "string" } },
                                { "type": "string" }
                            ]
                        },
                        "tone": { "type": "string", "description": "Optional writing tone (for example: formal, concise, persuasive)." },
                        "target_words": { "type": "integer", "description": "Optional target word count." },
                        "citation_keys": { "type": "array", "items": { "type": "string" }, "description": "Optional citation keys to weave into the draft." },
                        "output_format": { "type": "string", "description": "Optional output format: markdown | latex | plain." }
                    },
                    "required": ["section_type", "key_points"],
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "restructure_outline",
                "Restructure a manuscript outline into a coherent section order with rationale per section.",
                json!({
                    "type": "object",
                    "properties": {
                        "outline": { "type": "string", "description": "Optional free-form outline text." },
                        "sections": { "type": "array", "items": { "type": "string" }, "description": "Optional explicit section list." },
                        "manuscript_type": { "type": "string", "description": "Optional manuscript type: imrad | review | case_report | methods." }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "check_consistency",
                "Run consistency checks on manuscript text: abbreviations, numbering, terminology, placeholders, and citation marker style.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional project-relative path of the manuscript file." },
                        "text": { "type": "string", "description": "Optional inline manuscript text if path is not provided." }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "generate_abstract",
                "Generate a draft abstract from manuscript text with optional structured mode and word limit.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional project-relative manuscript path." },
                        "text": { "type": "string", "description": "Optional inline manuscript text if path is not provided." },
                        "structured": { "type": "boolean", "description": "Whether to output Background/Methods/Results/Conclusions sections." },
                        "word_limit": { "type": "integer", "description": "Optional word limit for the abstract." }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "insert_citation",
                "Insert a citation marker into text using a provided citation key and style (latex, markdown, or vancouver-like bracket).",
                json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "Target text where citation should be inserted." },
                        "citation_key": { "type": "string", "description": "Primary citation key to insert." },
                        "citation_keys": { "type": "array", "items": { "type": "string" }, "description": "Optional fallback citation keys; first non-empty key is used." },
                        "style": { "type": "string", "description": "Optional style: latex | markdown | vancouver." },
                        "placement": { "type": "string", "description": "Optional placement mode: sentence_end | append." },
                        "dedupe": { "type": "boolean", "description": "If true, avoid inserting duplicate markers already present in text." }
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "review_manuscript",
                "Perform a structured peer-review scan on manuscript text or file/document path and return severity-tagged findings.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional project-relative manuscript path (supports PDF/DOCX via document runtime)." },
                        "text": { "type": "string", "description": "Optional inline manuscript text if path is not provided." },
                        "focus": { "type": "string", "description": "Optional review focus (e.g., novelty, methods rigor, clarity)." },
                        "checklist": {
                            "description": "Optional checklist tags such as CONSORT/PRISMA/STROBE.",
                            "oneOf": [
                                { "type": "array", "items": { "type": "string" } },
                                { "type": "string" }
                            ]
                        }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "check_statistics",
                "Check statistical reporting quality in manuscript text and flag unsupported significance claims or missing uncertainty reporting.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional project-relative manuscript path." },
                        "text": { "type": "string", "description": "Optional inline manuscript text if path is not provided." }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "verify_references",
                "Verify internal citation-style consistency and detect potentially uncited narrative claims.",
                json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Optional project-relative manuscript path." },
                        "text": { "type": "string", "description": "Optional inline manuscript text if path is not provided." }
                    },
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "generate_response_letter",
                "Generate a point-by-point response letter draft from reviewer comments and optional revision plan notes.",
                json!({
                    "type": "object",
                    "properties": {
                        "reviewer_comments": {
                            "description": "Reviewer comments as an array or newline-delimited string.",
                            "oneOf": [
                                { "type": "array", "items": { "type": "string" } },
                                { "type": "string" }
                            ]
                        },
                        "revision_plan": { "type": "string", "description": "Optional high-level revision summary." },
                        "tone": { "type": "string", "description": "Optional response tone (default professional)." }
                    },
                    "required": ["reviewer_comments"],
                    "additionalProperties": false
                }),
            ),
            make_tool_spec(
                "track_revisions",
                "Track revision delta between old and new text (inline or file paths) and summarize changed line/word counts.",
                json!({
                    "type": "object",
                    "properties": {
                        "old_text": { "type": "string", "description": "Old manuscript text." },
                        "new_text": { "type": "string", "description": "New manuscript text." },
                        "old_path": { "type": "string", "description": "Path to old version file if old_text is omitted." },
                        "new_path": { "type": "string", "description": "Path to new version file if new_text is omitted." }
                    },
                    "additionalProperties": false
                }),
            ),
        ]);
    }

    specs.extend([
        make_tool_spec(
            "replace_selected_text",
            "Replace the currently selected text in a project file without rewriting the rest of the file. Use this for selection-scoped edits. REQUIREMENT: expected_selected_text must match the selected file content exactly, including whitespace and line breaks. If a selection_anchor is present, pass it exactly as provided in the prompt context. Do not use this tool for multi-location or whole-file rewrites.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative file path." },
                    "expected_selected_text": { "type": "string", "description": "Exact text expected to be selected in the file." },
                    "replacement_text": { "type": "string", "description": "Replacement text for the selected span." },
                    "selection_anchor": { "type": "string", "description": "Optional selection anchor like @path:startLine:startCol-endLine:endCol." }
                },
                "required": ["path", "expected_selected_text", "replacement_text"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "apply_text_patch",
            "Apply a precise text patch to a file. REQUIREMENT: expected_old_text must match the file content character-for-character, including spaces, indentation, and newlines. BEFORE CALLING: use read_file to retrieve the exact current content. DO NOT paraphrase, shorten, or reformat expected_old_text. If the text appears in multiple places, include more surrounding lines so the match is unique. Use write_file only for whole-file rewrites or new files.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative file path." },
                    "expected_old_text": { "type": "string", "description": "Exact text that must already exist in the file." },
                    "new_text": { "type": "string", "description": "Replacement text." }
                },
                "required": ["path", "expected_old_text", "new_text"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "write_file",
            "Write full replacement content to a file in the current project. Use this only for whole-file rewrites, creating a new file, or final apply steps after review. Do not use write_file for selection-scoped edits or narrow paragraph patches.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative file path." },
                    "content": { "type": "string", "description": "Full replacement file content." }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "list_files",
            "List files inside the current project.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional subdirectory inside the project." }
                },
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "search_project",
            "Search for text in the current project using ripgrep.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Text or regex query." },
                    "path": { "type": "string", "description": "Optional subdirectory inside the project." }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "run_shell_command",
            "Run a shell command in the current project. Use this for explicit engineering or environment tasks, not for default document reading or attachment extraction.",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to run." }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        ),
        make_tool_spec(
            "remember_fact",
            "Save an important fact to persistent memory. Use this to remember user preferences, project conventions, corrections, or key findings that should survive across sessions.",
            json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "The fact or preference to remember." },
                    "memory_type": { "type": "string", "enum": ["user_preference", "project_convention", "correction", "reference"], "description": "Category of the memory entry." },
                    "topic": { "type": "string", "description": "Optional topic tag for grouping related memories." }
                },
                "required": ["content"],
                "additionalProperties": false
            }),
        ),
    ]);

    specs
}

pub fn to_openai_tool_schema(spec: &AgentToolSpec) -> Value {
    let _ = spec.contract;
    json!({
        "type": "function",
        "name": spec.name,
        "description": spec.description,
        "parameters": adapt_tool_input_schema_for_provider(&spec.input_schema, "openai"),
    })
}

pub fn to_chat_completions_tool_schema(spec: &AgentToolSpec, provider: &str) -> Value {
    let _ = spec.contract;
    json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": adapt_tool_input_schema_for_provider(&spec.input_schema, provider),
        }
    })
}

pub fn parse_tool_arguments(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        match value {
            Value::Object(_) | Value::Array(_) => return Ok(value),
            Value::String(encoded) => {
                if let Ok(parsed) = serde_json::from_str::<Value>(&encoded) {
                    if matches!(parsed, Value::Object(_) | Value::Array(_)) {
                        return Ok(parsed);
                    }
                }
            }
            _ => {}
        }
    }

    if let Ok(encoded) = serde_json::from_str::<String>(raw) {
        if let Ok(value) = serde_json::from_str::<Value>(&encoded) {
            return Ok(value);
        }
    }

    if let Some(candidate) = extract_first_json_block(raw) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Ok(value);
        }
    }

    serde_json::from_str::<Value>(raw)
}

fn adapt_tool_input_schema_for_provider(schema: &Value, provider: &str) -> Value {
    let mut adapted = schema.clone();
    if matches!(provider, "minimax" | "deepseek") {
        strip_additional_properties_false(&mut adapted);
    }
    adapted
}

fn strip_additional_properties_false(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("additionalProperties") == Some(&Value::Bool(false)) {
                map.remove("additionalProperties");
            }
            for nested in map.values_mut() {
                strip_additional_properties_false(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_additional_properties_false(item);
            }
        }
        _ => {}
    }
}

fn extract_first_json_block(raw: &str) -> Option<&str> {
    let start = raw.find('{').or_else(|| raw.find('['))?;
    let bytes = raw.as_bytes();
    let opener = bytes.get(start).copied()?;
    let closer = if opener == b'{' { b'}' } else { b']' };

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;

    for (idx, byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if *byte == b'\\' {
                escape = true;
                continue;
            }
            if *byte == b'"' {
                in_string = false;
            }
            continue;
        }

        if *byte == b'"' {
            in_string = true;
            continue;
        }
        if *byte == opener {
            depth += 1;
            continue;
        }
        if *byte == closer {
            depth -= 1;
            if depth == 0 {
                return raw.get(start..=idx);
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

pub fn truncate_preview(value: &str) -> String {
    if value.chars().count() > MAX_PREVIEW_CHARS {
        format!(
            "{}...",
            value.chars().take(MAX_PREVIEW_CHARS).collect::<String>()
        )
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
    [
        "path",
        "file_path",
        "targetPath",
        "filePath",
        "command",
        "query",
    ]
    .iter()
    .find_map(|key| content.get(*key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_string)
}

pub fn tool_result_requires_approval(result: &AgentToolResult) -> bool {
    result
        .content
        .get("approvalRequired")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn tool_result_review_ready(result: &AgentToolResult) -> bool {
    result
        .content
        .get("reviewArtifact")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

pub fn tool_result_display_content(result: &AgentToolResult) -> AgentToolResultDisplayContent {
    let review_artifact_payload = result
        .content
        .get("reviewArtifactPayload")
        .and_then(Value::as_object);
    let target_path = summarize_tool_target(&result.content).or_else(|| {
        review_artifact_payload.and_then(|payload| {
            payload
                .get("targetPath")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    });
    let command = result
        .content
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            result
                .content
                .get("input")
                .and_then(Value::as_object)
                .and_then(|payload| payload.get("command"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let query = result
        .content
        .get("query")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            result
                .content
                .get("input")
                .and_then(Value::as_object)
                .and_then(|payload| payload.get("query"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let approval_required = tool_result_requires_approval(result);
    let review_ready = tool_result_review_ready(result);
    let approval_reason = result
        .content
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    let approval_tool_name = result
        .content
        .get("approvalToolName")
        .and_then(Value::as_str)
        .or_else(|| result.content.get("toolName").and_then(Value::as_str))
        .map(str::to_string);
    let summary = review_artifact_payload
        .and_then(|payload| payload.get("summary"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            result
                .content
                .get("summary")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let content_text = result
        .content
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string);
    let error_text = result
        .content
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string);
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
        text_preview: error_text
            .or(content_text)
            .or(approval_reason.clone())
            .or(summary.clone())
            .unwrap_or_else(|| result.preview.clone()),
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

    if matches!(
        context.task_kind,
        AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview
    ) && context.has_binary_attachment_context
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

    if matches!(
        context.task_kind,
        AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview
    ) && context.has_binary_attachment_context
        && call.tool_name == "read_file"
        && target.map(is_document_resource_path).unwrap_or(false)
    {
        return Some(AgentToolResult {
            tool_name: call.tool_name.clone(),
            call_id: call.call_id.clone(),
            is_error: true,
            preview:
                "Binary attachment analysis must rely on ingested excerpts, not raw file reads."
                    .to_string(),
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

#[cfg(test)]
mod tests {
    use super::{
        build_default_tool_specs, default_tool_specs, parse_tool_arguments,
        to_chat_completions_tool_schema, to_openai_tool_schema, tool_contract, AgentToolSpec,
    };
    use serde_json::json;

    #[test]
    fn maps_tool_spec_to_openai_function_shape() {
        let tool = AgentToolSpec {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: json!({"type": "object"}),
            contract: tool_contract("read_file"),
        };
        let mapped = to_openai_tool_schema(&tool);
        assert_eq!(mapped["type"], "function");
        assert_eq!(mapped["name"], "read_file");
        assert_eq!(mapped["description"], "Read a file");
        assert_eq!(mapped["parameters"]["type"], "object");
    }

    #[test]
    fn chat_completions_schema_strips_additional_properties_for_minimax_and_deepseek() {
        let tool = AgentToolSpec {
            name: "demo_tool".to_string(),
            description: "Demo".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "payload": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["payload"],
                "additionalProperties": false
            }),
            contract: tool_contract("read_file"),
        };

        let minimax = to_chat_completions_tool_schema(&tool, "minimax");
        let deepseek = to_chat_completions_tool_schema(&tool, "deepseek");
        let openai = to_chat_completions_tool_schema(&tool, "openai");

        assert!(minimax["function"]["parameters"]
            .get("additionalProperties")
            .is_none());
        assert!(deepseek["function"]["parameters"]
            .get("additionalProperties")
            .is_none());
        assert_eq!(
            openai["function"]["parameters"]["additionalProperties"],
            json!(false)
        );
        assert!(minimax["function"]["parameters"]["properties"]["payload"]
            .get("additionalProperties")
            .is_none());
    }

    #[test]
    fn parse_tool_arguments_recovers_json_wrapped_string() {
        let parsed =
            parse_tool_arguments("\"{\\\"path\\\":\\\"main.tex\\\",\\\"query\\\":\\\"intro\\\"}\"")
                .expect("wrapped JSON should parse");
        assert_eq!(parsed["path"], "main.tex");
        assert_eq!(parsed["query"], "intro");
    }

    #[test]
    fn parse_tool_arguments_recovers_json_from_wrapped_text() {
        let parsed = parse_tool_arguments(
            "```json\n{\"path\":\"main.tex\",\"expected_old_text\":\"a\",\"new_text\":\"b\"}\n```",
        )
        .expect("mixed text JSON should parse");
        assert_eq!(parsed["path"], "main.tex");
        assert_eq!(parsed["new_text"], "b");
    }

    #[test]
    fn exposes_single_document_tool_entry_in_default_specs() {
        let names = default_tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "read_document"));
        assert!(!names.iter().any(|name| name == "inspect_resource"));
        assert!(!names.iter().any(|name| name == "read_document_excerpt"));
        assert!(!names.iter().any(|name| name == "search_document_text"));
        assert!(!names.iter().any(|name| name == "get_document_evidence"));
    }

    #[test]
    fn includes_writing_tools_when_enabled() {
        let names = build_default_tool_specs(true)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "draft_section"));
        assert!(names.iter().any(|name| name == "restructure_outline"));
        assert!(names.iter().any(|name| name == "check_consistency"));
        assert!(names.iter().any(|name| name == "generate_abstract"));
        assert!(names.iter().any(|name| name == "insert_citation"));
    }

    #[test]
    fn excludes_writing_tools_when_disabled() {
        let names = build_default_tool_specs(false)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name == "draft_section"));
        assert!(!names.iter().any(|name| name == "restructure_outline"));
        assert!(!names.iter().any(|name| name == "check_consistency"));
        assert!(!names.iter().any(|name| name == "generate_abstract"));
        assert!(!names.iter().any(|name| name == "insert_citation"));
    }
}
