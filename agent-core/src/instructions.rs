use crate::config::AgentRuntimeConfig;
use crate::provider::{
    AgentResponseMode, AgentSamplingProfile, AgentSelectionScope, AgentTaskKind,
    AgentTurnDescriptor, AgentTurnProfile,
};
use crate::session::AgentSessionWorkState;

const AGENT_BASE_INSTRUCTIONS: &str = concat!(
    "You are an AI assistant integrated into ChatPrism, a project-aware academic writing and coding workspace. ",
    "Behave like an execution-oriented agent, not a general chat assistant.\n",
    "\n",
    "[Context marker meaning]\n",
    "- [Currently open file: X] means the user is actively working in file X.\n",
    "- [Selection: @X:startLine:startCol-endLine:endCol] means the user selected a span inside file X. Treat the full @... string as the exact selection anchor.\n",
    "- [Selected text: ...] means the user selected that exact text. When using selection-edit tools, preserve it verbatim unless you have re-read the file and verified a different exact match.\n",
    "- [Attached resource: X] means the user attached or pinned a reference resource. It is supporting context, not an active editor selection.\n",
    "- [Resource path: X] gives the project-relative path for an attached resource.\n",
    "- [Attached excerpt: ...] contains a quoted excerpt or extracted text from an attached resource. Treat it as reference material for analysis, extraction, or comparison tasks.\n",
    "- [Relevant resource matches: ...] or [Relevant resource evidence: ...] contains local search hits extracted from attached resources. Treat these hits as high-signal evidence candidates and use them before resorting to shell probing or repeated exploration.\n",
    "\n",
    "[Hard execution rules]\n",
    "1. When the request is an edit or file-work request, take concrete project actions instead of only replying with prose.\n",
    "2. Keep edits small and targeted. Preserve surrounding structure, formatting, and unrelated content.\n",
    "3. If the request targets selected text, you MUST treat it as a selection-scoped edit task unless the user explicitly asks for suggestions only.\n",
    "4. For selection-scoped edits, use precise edit tools such as replace_selected_text or apply_text_patch. Using write_file for a selection-scoped edit is forbidden unless the user explicitly asks for a whole-file rewrite.\n",
    "5. For file-scoped edits without a trusted exact target, read the text file first, then patch it with exact existing text. Do not guess exact text spans.\n",
    "6. Use write_file only for whole-file rewrites, creating a new file, or final apply steps after review.\n",
    "7. If the user explicitly asks for suggestions, analysis, explanation, review-only output, or says not to modify files, stay in suggestion mode and do not call edit tools.\n",
    "8. If a write or shell action is blocked by approval, do not pretend the change was applied. Treat it as staged/pending and wait.\n",
    "9. Do not treat attached resources as active-file selections unless the prompt also contains an explicit [Selection: ...] marker.\n",
    "10. PDFs and DOCX resources are documents, not plain text files. Use read_document instead of read_file when you need evidence from an attached document.\n",
    "\n",
    "[Internal tool-use checklist]\n",
    "Before every tool call, verify internally:\n",
    "- which file you are acting on\n",
    "- whether you already read the current file content when exact matching is required\n",
    "- whether expected_old_text or expected_selected_text matches the current file verbatim, including whitespace and line breaks\n",
    "Do not reveal this checklist unless the user asks for your reasoning.\n",
    "\n",
    "[Response style]\n",
    "Keep responses concise when the real value is in the tool action and resulting reviewable change.\n"
);

const BIOMEDICAL_DOMAIN_INSTRUCTIONS: &str = concat!(
    "[Biomedical domain guardrails]\n",
    "- Use biomedical terminology precisely (gene/protein naming, disease entities, intervention names).\n",
    "- Do not fabricate citations, datasets, outcomes, statistical values, or trial details.\n",
    "- When discussing evidence quality, prefer explicit study-design framing (systematic review, RCT, cohort, case-control, case series, expert opinion).\n",
    "- When claims involve statistics, check for explicit support (effect size, confidence interval, p-value, or equivalent) before making strong conclusions.\n",
    "- If evidence is incomplete, lower certainty language instead of over-claiming.\n",
    "- Flag possible ethics/reporting gaps when relevant (IRB/ethics approval, conflicts, CONSORT/PRISMA/STROBE-style reporting expectations).\n",
);

fn prompt_contains_cjk(prompt: &str) -> bool {
    prompt
        .chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
}

fn provider_adaptive_instruction_block(
    provider: &str,
    request: &AgentTurnDescriptor,
    profile: &AgentTurnProfile,
) -> Option<String> {
    match provider {
        "openai" => Some(
            "[Provider operating note]\n\
            - Keep tool arguments strict JSON with all required fields.\n\
            - For analysis/review answers, use compact structure (short heading + bullets) when it improves clarity.\n"
                .to_string(),
        ),
        "deepseek" => Some(
            "[Provider operating note]\n\
            - Think step-by-step before choosing tools.\n\
            - Before finalizing, cross-check conclusions against extracted evidence and call out uncertainty explicitly.\n"
                .to_string(),
        ),
        "minimax" => {
            let mut block = String::from(
                "[Provider operating note]\n\
                - Maintain strong reasoning depth. For analysis/literature/peer-review tasks, do not stop at a one-line conclusion; provide at least 3 evidence-backed points before final takeaway.\n\
                - Validate tool outputs against the user question before producing final claims.\n\
                - After a successful edit (tool returns 'Edit applied successfully'), do NOT run shell commands to verify the edit. Trust the tool result and summarize what was changed.\n\
                - Do not re-read a file after you just edited it unless the user asks for another change.\n\
                - Avoid repetitive tool calls: if you already read a file, do not read it again. If you already ran a command, do not run it again with minor variations.\n\
                - When your edit is complete, respond with a brief summary to the user. Do not keep calling tools.\n",
            );
            if matches!(
                profile.task_kind,
                AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview
            ) {
                block.push_str(
                    "- For complex judgments, include: evidence summary, limits/uncertainty, and actionable next step.\n",
                );
            }
            if prompt_contains_cjk(&request.prompt) {
                block.push_str(
                    "- 用户使用中文时优先中文输出；保留专业术语原文并给出必要解释。\n",
                );
            }
            Some(block)
        }
        _ => None,
    }
}

fn prompt_lower(prompt: &str) -> String {
    prompt.to_lowercase()
}

fn has_selection_context(prompt: &str) -> bool {
    prompt.contains("[Selection:")
}

fn has_attachment_context(prompt: &str) -> bool {
    prompt.contains("[Attached resource:")
}

fn has_binary_attachment_context(prompt: &str) -> bool {
    prompt.lines().any(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("[Resource path: ") || !trimmed.ends_with(']') {
            return false;
        }
        let path = trimmed
            .trim_start_matches("[Resource path: ")
            .trim_end_matches(']')
            .trim()
            .to_ascii_lowercase();
        path.ends_with(".pdf") || path.ends_with(".docx")
    })
}

fn prompt_explicitly_requests_suggestions(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "suggest",
        "give me a version",
        "show me a version",
        "propose",
        "brainstorm",
        "review only",
        "do not modify",
        "don't modify",
        "without editing",
        "without changing the file",
    ];
    let zh = [
        "建议",
        "给我几个",
        "有没有更好",
        "可以怎么",
        "怎么改比较好",
        "不要改文件",
        "只是看看",
        "解释一下",
        "分析一下",
        "仅建议",
        "只做建议",
        "只看一下",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_explicitly_requests_edit(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "refine",
        "rewrite",
        "edit",
        "polish",
        "improve",
        "fix",
        "revise",
        "shorten",
        "tighten",
        "rephrase",
        "proofread",
        "clean up",
    ];
    let zh = [
        "修改",
        "改成",
        "改为",
        "润色",
        "优化",
        "精简",
        "修正",
        "完善",
        "重写",
        "调整",
        "修一下",
        "帮我改",
        "改一下",
        "润一下",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_explicitly_requests_deep_analysis(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "detailed",
        "deep",
        "deeper",
        "compare",
        "comparison",
        "synthesize",
        "summary",
        "summarize",
        "which paper",
        "which article",
        "evidence",
        "walk me through",
    ];
    let zh = [
        "详细",
        "深入",
        "展开",
        "对比",
        "比较",
        "总结",
        "归纳",
        "综述",
        "哪篇",
        "哪一篇",
        "列出",
        "依据",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_requests_literature_review(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "review literature",
        "literature review",
        "related work",
        "find papers",
        "what does the literature say",
        "survey papers",
        "evidence synthesis",
    ];
    let zh = [
        "文献综述",
        "相关研究",
        "找文献",
        "文献调研",
        "研究现状",
        "检索文献",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_requests_paper_drafting(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "draft section",
        "draft introduction",
        "draft methods",
        "draft results",
        "draft discussion",
        "write introduction",
        "write methods",
        "write discussion",
        "generate abstract",
        "manuscript",
    ];
    let zh = [
        "写引言",
        "撰写引言",
        "撰写方法",
        "撰写结果",
        "撰写讨论",
        "草拟讨论",
        "写摘要",
        "论文草稿",
        "论文写作",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_requests_peer_review(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "peer review",
        "review this manuscript",
        "review this paper",
        "review comments",
        "check for issues",
        "response letter",
    ];
    let zh = ["审稿", "审查论文", "评审意见", "找问题", "回复审稿人"];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn prompt_explicitly_requests_chinese_output(prompt: &str) -> bool {
    let lower = prompt_lower(prompt);
    let en = [
        "in chinese",
        "respond in chinese",
        "answer in chinese",
        "use chinese",
    ];
    let zh = [
        "用中文",
        "中文回答",
        "请用中文",
        "请中文",
        "使用中文",
        "中文输出",
    ];
    en.iter().any(|needle| lower.contains(needle))
        || zh.iter().any(|needle| prompt.contains(needle))
}

fn has_relevant_resource_evidence(prompt: &str) -> bool {
    prompt.contains("[Relevant resource evidence:")
        || prompt.contains("[Relevant resource matches:")
}

pub fn tool_choice_for_task(
    request: &AgentTurnDescriptor,
    profile: &AgentTurnProfile,
) -> &'static str {
    match profile.task_kind {
        AgentTaskKind::SelectionEdit | AgentTaskKind::FileEdit => "required",
        AgentTaskKind::SuggestionOnly => "none",
        AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview
            if has_attachment_context(&request.prompt)
                && has_relevant_resource_evidence(&request.prompt) =>
        {
            "none"
        }
        AgentTaskKind::Analysis | AgentTaskKind::LiteratureReview | AgentTaskKind::PeerReview
            if has_binary_attachment_context(&request.prompt) =>
        {
            "required"
        }
        AgentTaskKind::General
        | AgentTaskKind::Analysis
        | AgentTaskKind::LiteratureReview
        | AgentTaskKind::PaperDrafting
        | AgentTaskKind::PeerReview => "auto",
    }
}

pub fn max_rounds_for_task(profile: &AgentTurnProfile) -> u32 {
    match profile.task_kind {
        AgentTaskKind::SuggestionOnly => 2,
        AgentTaskKind::SelectionEdit => 12,
        AgentTaskKind::FileEdit => 25,
        AgentTaskKind::LiteratureReview => 30,
        AgentTaskKind::PaperDrafting => 25,
        AgentTaskKind::PeerReview => 20,
        AgentTaskKind::Analysis | AgentTaskKind::General => 25,
    }
}

pub fn resolve_turn_profile(request: &AgentTurnDescriptor) -> AgentTurnProfile {
    let mut profile = request.turn_profile.clone().unwrap_or_default();

    if profile.selection_scope == AgentSelectionScope::None
        && has_selection_context(&request.prompt)
    {
        profile.selection_scope = AgentSelectionScope::SelectedSpan;
    }

    if profile.task_kind == AgentTaskKind::General {
        if profile.selection_scope == AgentSelectionScope::SelectedSpan {
            if prompt_explicitly_requests_suggestions(&request.prompt) {
                profile.task_kind = AgentTaskKind::SuggestionOnly;
            } else if prompt_explicitly_requests_edit(&request.prompt) {
                profile.task_kind = AgentTaskKind::SelectionEdit;
            }
        } else if prompt_requests_peer_review(&request.prompt) {
            profile.task_kind = AgentTaskKind::PeerReview;
        } else if prompt_requests_paper_drafting(&request.prompt) {
            profile.task_kind = AgentTaskKind::PaperDrafting;
        } else if prompt_requests_literature_review(&request.prompt) {
            profile.task_kind = AgentTaskKind::LiteratureReview;
        } else if has_attachment_context(&request.prompt) {
            if prompt_explicitly_requests_suggestions(&request.prompt) {
                profile.task_kind = AgentTaskKind::SuggestionOnly;
            } else {
                profile.task_kind = AgentTaskKind::Analysis;
            }
        } else if prompt_explicitly_requests_suggestions(&request.prompt) {
            profile.task_kind = AgentTaskKind::SuggestionOnly;
        }
    }

    if profile.response_mode == AgentResponseMode::Default {
        profile.response_mode = match profile.task_kind {
            AgentTaskKind::SelectionEdit | AgentTaskKind::FileEdit => {
                AgentResponseMode::ReviewableChange
            }
            AgentTaskKind::SuggestionOnly => AgentResponseMode::SuggestionOnly,
            AgentTaskKind::General
            | AgentTaskKind::Analysis
            | AgentTaskKind::LiteratureReview
            | AgentTaskKind::PaperDrafting
            | AgentTaskKind::PeerReview => AgentResponseMode::Default,
        };
    }

    if profile.sampling_profile == AgentSamplingProfile::Default {
        profile.sampling_profile = match profile.task_kind {
            AgentTaskKind::SelectionEdit | AgentTaskKind::FileEdit => {
                AgentSamplingProfile::EditStable
            }
            AgentTaskKind::SuggestionOnly => AgentSamplingProfile::AnalysisBalanced,
            AgentTaskKind::Analysis
            | AgentTaskKind::LiteratureReview
            | AgentTaskKind::PaperDrafting
            | AgentTaskKind::PeerReview => {
                if has_attachment_context(&request.prompt)
                    || prompt_explicitly_requests_deep_analysis(&request.prompt)
                {
                    AgentSamplingProfile::AnalysisDeep
                } else {
                    AgentSamplingProfile::AnalysisBalanced
                }
            }
            AgentTaskKind::General => AgentSamplingProfile::Default,
        };
    }

    profile
}

pub fn build_agent_instructions_with_work_state(
    request: &AgentTurnDescriptor,
    work_state: Option<&AgentSessionWorkState>,
    runtime_config: Option<&AgentRuntimeConfig>,
    memory_context: Option<&str>,
) -> String {
    let mut instructions = AGENT_BASE_INSTRUCTIONS.to_string();
    let profile = resolve_turn_profile(request);

    if prompt_explicitly_requests_chinese_output(&request.prompt) {
        instructions.push_str(
            "Output language policy: respond in Chinese for this turn because the user explicitly requested Chinese output.\n",
        );
    } else {
        instructions.push_str(
            "Output language policy: respond in English by default. Only switch to Chinese when the user explicitly asks for Chinese output.\n",
        );
    }

    if let Some(runtime) = runtime_config {
        if runtime.domain_config.domain == "biomedical" {
            instructions.push('\n');
            instructions.push_str(BIOMEDICAL_DOMAIN_INSTRUCTIONS);
            instructions.push('\n');
        }
        instructions.push_str(&format!(
            "Domain terminology strictness: {}.\n",
            runtime.domain_config.terminology_strictness
        ));
        if let Some(custom) = runtime.domain_config.custom_instructions.as_deref() {
            instructions.push_str("[Custom domain instructions]\n");
            instructions.push_str(custom);
            if !custom.ends_with('\n') {
                instructions.push('\n');
            }
        }

        if let Some(provider_block) =
            provider_adaptive_instruction_block(&runtime.provider, request, &profile)
        {
            instructions.push_str(&provider_block);
            if !provider_block.ends_with('\n') {
                instructions.push('\n');
            }
        }
    }

    if let Some(mem) = memory_context {
        if !mem.is_empty() {
            instructions.push('\n');
            instructions.push_str(mem);
            if !mem.ends_with('\n') {
                instructions.push('\n');
            }
        }
    }

    match profile.task_kind {
        AgentTaskKind::SelectionEdit => {
            instructions.push_str(
                "This turn is classified as a selection-scoped edit task. \
You must aim for a reviewable file change, not a prose-only rewritten paragraph. \
Use replace_selected_text when the selected text and selection anchor are trustworthy. \
If you need to verify the exact file content first, call read_file, then use apply_text_patch with an exact verbatim match. \
Do not use write_file for this selection-scoped edit unless the user explicitly asks for a whole-file rewrite.\n",
            );
        }
        AgentTaskKind::FileEdit => {
            instructions.push_str(
                "This turn is classified as a file-edit request. \
Produce a reviewable file change instead of a prose-only explanation whenever possible. \
Read the file before exact-match patching, and reserve write_file for whole-file rewrites or final apply steps.\n",
            );
        }
        AgentTaskKind::SuggestionOnly => {
            instructions.push_str(
                "This turn is classified as suggestion-only. \
Stay in suggestion mode and avoid edit tools unless the user explicitly asks to apply the change.\n",
            );
        }
        AgentTaskKind::Analysis => {
            instructions.push_str(
                "This turn is classified as analysis. Prefer clear reasoning and targeted file reads over file edits unless the user explicitly asks for changes.\n",
            );
        }
        AgentTaskKind::LiteratureReview => {
            instructions.push_str(
                "This turn is classified as literature review. Build evidence-grounded synthesis, distinguish study designs, and clearly separate findings from uncertainty.\n",
            );
        }
        AgentTaskKind::PaperDrafting => {
            instructions.push_str(
                "This turn is classified as paper drafting. Produce structured manuscript-ready prose, preserve scientific caution, and keep section-level coherence.\n",
            );
        }
        AgentTaskKind::PeerReview => {
            instructions.push_str(
                "This turn is classified as peer review. Prioritize actionable findings with severity labels and tie each critique to concrete evidence from the manuscript/resources.\n",
            );
        }
        AgentTaskKind::General => {
            if profile.selection_scope == AgentSelectionScope::SelectedSpan {
                instructions.push_str(
                    "This turn includes selected text context. Treat the selection as high-signal context, but only perform file edits when the request clearly calls for modification.\n",
                );
            }
        }
    }

    if has_attachment_context(&request.prompt) {
        instructions.push_str(
            "This turn includes attached resources. Ground your answer in those resources, cite which attached file supports each key conclusion, and synthesize evidence before concluding.\n",
        );
        instructions.push_str(
            "When answering a document/resource question, prefer this structure when it fits: Matching documents, Supporting evidence (cite the attached file plus the page or paragraph label from the evidence block), then Conclusion. If the ingested evidence is insufficient, say that clearly instead of inventing tool calls or shell steps.\n",
        );
        if has_binary_attachment_context(&request.prompt) {
            instructions.push_str(
                "For attached PDFs or DOCX resources, use read_document when additional evidence is needed. Do not call read_file on binary files. Do not use shell commands such as pdftotext for exploratory extraction unless the user explicitly asks for command-line inspection. If read_document reports fallback extraction, treat it as runtime-managed evidence gathering.\n",
            );
            instructions.push_str(
                "[Document analysis strategy]\n\
                When the user asks you to analyze, summarize, extract information from, or answer questions about attached PDF or DOCX documents:\n\
                1. First call inspect_resource to check document metadata and extraction status.\n\
                2. For summary/overview tasks, start with read_document (with or without a query) to get the excerpt.\n\
                3. For specific information queries, call search_document_text with relevant keywords. Search multiple times with different keywords if needed to build a complete picture.\n\
                4. Do NOT rely solely on pre-extracted excerpts. Proactively search for key topics, main points, methods, results, conclusions, recommendations based on what the user is asking.\n\
                5. If search returns no results for a term, try alternative keywords or broader searches.\n\
                6. Synthesize all found evidence into a comprehensive answer that directly addresses the user's question.\n"
            );
        }
    }

    if profile.sampling_profile == AgentSamplingProfile::AnalysisDeep {
        instructions.push_str(
            "Use a deeper analysis style for this turn: inspect evidence carefully, compare relevant sources when useful, and avoid stopping at a one-line conclusion when the user is asking a research or document question.\n",
        );
    }

    if let Some(work_state) = work_state {
        let recall_lines = selective_session_recall(request, work_state);
        if !recall_lines.is_empty() {
            if !instructions.ends_with('\n') {
                instructions.push('\n');
            }
            instructions.push_str("[Selective session recall]\n");
            instructions.push_str(
                "Use these continuity hints to stay aligned with the active task and avoid repeating already completed exploration unless the current request truly requires it.\n",
            );
            for line in recall_lines {
                instructions.push_str("- ");
                instructions.push_str(&line);
                instructions.push('\n');
            }
            instructions.push('\n');
        }
    }

    instructions
}

fn push_unique_recall_line(lines: &mut Vec<String>, line: Option<String>) {
    let Some(line) = line.map(|value| value.trim().to_string()) else {
        return;
    };
    if line.is_empty() || lines.iter().any(|existing| existing == &line) {
        return;
    }
    lines.push(line);
}

fn selective_session_recall(
    request: &AgentTurnDescriptor,
    work_state: &AgentSessionWorkState,
) -> Vec<String> {
    let profile = resolve_turn_profile(request);
    let current_request_objective = summarize_objective(&request.prompt);
    let mut lines = Vec::new();

    if let Some(pending_state) = work_state.pending_state.as_deref() {
        let pending_tool = work_state.pending_tool_name.as_deref().unwrap_or("tool");
        let pending_target = work_state
            .pending_target
            .as_deref()
            .map(|value| format!(" on {}", value))
            .unwrap_or_default();
        push_unique_recall_line(
            &mut lines,
            Some(format!(
                "Pending state: {} via {}{}",
                pending_state, pending_tool, pending_target
            )),
        );
    }

    if let Some(recent_objective) = work_state.recent_objective.as_deref() {
        push_unique_recall_line(
            &mut lines,
            Some(format!("Recent objective: {}", recent_objective)),
        );
    }

    let should_recall_target = matches!(
        profile.task_kind,
        AgentTaskKind::SelectionEdit
            | AgentTaskKind::FileEdit
            | AgentTaskKind::Analysis
            | AgentTaskKind::LiteratureReview
            | AgentTaskKind::PaperDrafting
            | AgentTaskKind::PeerReview
    ) || work_state.pending_state.is_some();
    if should_recall_target {
        if let Some(target) = work_state.current_target.as_deref() {
            push_unique_recall_line(&mut lines, Some(format!("Working target: {}", target)));
        }
    }

    if let Some(activity) = work_state.last_tool_activity.as_deref() {
        let should_include_activity = work_state.pending_state.is_some()
            || matches!(
                profile.task_kind,
                AgentTaskKind::Analysis
                    | AgentTaskKind::General
                    | AgentTaskKind::LiteratureReview
                    | AgentTaskKind::PaperDrafting
                    | AgentTaskKind::PeerReview
            )
            || lines.len() < 2;
        if should_include_activity {
            push_unique_recall_line(
                &mut lines,
                Some(format!("Recent tool activity: {}", activity)),
            );
        }
    }

    if let Some(workflow_type) = work_state.academic_workflow.workflow_type.as_deref() {
        let workflow_stage = work_state
            .academic_workflow
            .current_step
            .as_deref()
            .map(|value| format!(" at stage {}", value))
            .unwrap_or_default();
        push_unique_recall_line(
            &mut lines,
            Some(format!(
                "Workflow context: {}{}",
                workflow_type, workflow_stage
            )),
        );
    }

    if matches!(
        profile.task_kind,
        AgentTaskKind::LiteratureReview | AgentTaskKind::PaperDrafting | AgentTaskKind::PeerReview
    ) && !work_state.collected_references.is_empty()
    {
        push_unique_recall_line(
            &mut lines,
            Some(format!(
                "Reference memory: {} collected references available in this session.",
                work_state.collected_references.len()
            )),
        );
        let recent_titles = work_state
            .collected_references
            .iter()
            .rev()
            .take(2)
            .map(|entry| entry.title.trim())
            .filter(|title| !title.is_empty())
            .collect::<Vec<_>>();
        if !recent_titles.is_empty() {
            push_unique_recall_line(
                &mut lines,
                Some(format!("Recent references: {}", recent_titles.join("; "))),
            );
        }
    }

    if matches!(
        profile.task_kind,
        AgentTaskKind::PaperDrafting | AgentTaskKind::PeerReview
    ) {
        if !work_state.academic_workflow.review_findings.is_empty() {
            push_unique_recall_line(
                &mut lines,
                Some(format!(
                    "Review memory: {} findings captured for this session.",
                    work_state.academic_workflow.review_findings.len()
                )),
            );
        }
        if let Some(summary) = work_state
            .academic_workflow
            .revision_tracker
            .as_ref()
            .and_then(|snapshot| snapshot.summary.as_deref())
        {
            push_unique_recall_line(&mut lines, Some(format!("Revision tracker: {}", summary)));
        }
    }

    if let Some(objective) = work_state.current_objective.as_deref() {
        if current_request_objective.as_deref() != Some(objective) {
            push_unique_recall_line(&mut lines, Some(format!("Active objective: {}", objective)));
        }
    }

    if lines.len() > 4 {
        lines.truncate(4);
    }

    lines
}

pub fn summarize_objective(prompt: &str) -> Option<String> {
    prompt
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('['))
        .map(|line| {
            if line.chars().count() > 120 {
                format!("{}...", line.chars().take(120).collect::<String>())
            } else {
                line.to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{
        build_agent_instructions_with_work_state, max_rounds_for_task, resolve_turn_profile,
        summarize_objective, tool_choice_for_task,
    };
    use crate::{
        AgentDomainConfig, AgentResponseMode, AgentRuntimeConfig, AgentSamplingConfig,
        AgentSamplingProfile, AgentSamplingProfilesConfig, AgentSelectionScope,
        AgentSessionWorkState, AgentTaskKind, AgentTurnDescriptor, AgentTurnProfile,
    };

    fn make_request(prompt: &str, turn_profile: Option<AgentTurnProfile>) -> AgentTurnDescriptor {
        AgentTurnDescriptor {
            project_path: "/tmp/project".to_string(),
            prompt: prompt.to_string(),
            tab_id: "tab-1".to_string(),
            model: None,
            local_session_id: None,
            previous_response_id: None,
            turn_profile,
        }
    }

    fn make_runtime(domain: &str, custom: Option<&str>) -> AgentRuntimeConfig {
        AgentRuntimeConfig {
            runtime: "local_agent".to_string(),
            provider: "minimax".to_string(),
            model: "MiniMax-M2.5".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            api_key: None,
            domain_config: AgentDomainConfig {
                domain: domain.to_string(),
                custom_instructions: custom.map(str::to_string),
                terminology_strictness: "moderate".to_string(),
            },
            sampling_profiles: AgentSamplingProfilesConfig {
                edit_stable: AgentSamplingConfig {
                    temperature: 0.2,
                    top_p: 0.9,
                    max_tokens: 8192,
                },
                analysis_balanced: AgentSamplingConfig {
                    temperature: 0.4,
                    top_p: 0.9,
                    max_tokens: 6144,
                },
                analysis_deep: AgentSamplingConfig {
                    temperature: 0.3,
                    top_p: 0.92,
                    max_tokens: 12288,
                },
                chat_flexible: AgentSamplingConfig {
                    temperature: 0.7,
                    top_p: 0.95,
                    max_tokens: 4096,
                },
            },
        }
    }

    #[test]
    fn resolve_turn_profile_marks_selection_edit_for_explicit_edit_intent() {
        let request = make_request(
            "[Currently open file: main.tex]\n[Selection: @main.tex:14:1-14:20]\n[Selected text:\nfoo\n]\n\nrefine this paragraph",
            None,
        );

        let profile = resolve_turn_profile(&request);

        assert_eq!(profile.task_kind, AgentTaskKind::SelectionEdit);
        assert_eq!(profile.selection_scope, AgentSelectionScope::SelectedSpan);
        assert_eq!(profile.response_mode, AgentResponseMode::ReviewableChange);
    }

    #[test]
    fn tool_choice_requires_document_tools_for_binary_attachment_without_evidence() {
        let request = make_request(
            "[Attached resource: @paper.pdf (pdf)]\n[Resource path: attachments/paper.pdf]\n[Attached excerpt:\nhydrophobic surface treatment\n]\n\nWhich paper mentions hydrophobic experiments?",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::Analysis,
                ..AgentTurnProfile::default()
            }),
        );

        assert_eq!(
            tool_choice_for_task(&request, &resolve_turn_profile(&request)),
            "required"
        );
    }

    #[test]
    fn binary_attachment_with_evidence_does_not_force_extra_tool_turns() {
        let request = make_request(
            "[Attached resource: @paper.pdf (pdf)]\n[Resource path: attachments/paper.pdf]\n[Attached excerpt:\nhydrophobic surface treatment\n]\n[Relevant resource evidence:\n- Document: attachments/paper.pdf (pdf)\n  - Page 4: hydrophobic surface treatment was evaluated by contact angle measurements.\n]\n\nWhich paper mentions hydrophobic experiments?",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::Analysis,
                ..AgentTurnProfile::default()
            }),
        );

        assert_eq!(
            tool_choice_for_task(&request, &resolve_turn_profile(&request)),
            "none"
        );
    }

    #[test]
    fn selective_session_recall_prefers_recent_objective_over_current_prompt_echo() {
        let request = make_request(
            "[Attached resource: @paper.pdf]\n[Resource path: attachments/paper.pdf]\n[Attached excerpt:\nhydrophobic surface treatment\n]\n\nWhich paper mentions hydrophobic experiments?",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::Analysis,
                sampling_profile: AgentSamplingProfile::AnalysisDeep,
                ..AgentTurnProfile::default()
            }),
        );
        let work_state = AgentSessionWorkState {
            current_objective: Some("Which paper mentions hydrophobic experiments?".to_string()),
            recent_objective: Some(
                "Compare the attached papers for hydrophobic experiments.".to_string(),
            ),
            current_target: Some("attachments/TiO2 CuS PDA.pdf".to_string()),
            last_tool_activity: Some(
                "Completed run_shell_command on attachments/TiO2 CuS PDA.pdf".to_string(),
            ),
            pending_state: None,
            pending_tool_name: None,
            pending_target: None,
            collected_references: Vec::new(),
            academic_workflow: Default::default(),
        };

        let instructions =
            build_agent_instructions_with_work_state(&request, Some(&work_state), None, None);
        assert!(instructions.contains("[Selective session recall]"));
        assert!(instructions.contains(
            "Recent objective: Compare the attached papers for hydrophobic experiments."
        ));
        assert!(
            !instructions
                .contains("Active objective: Which paper mentions hydrophobic experiments?")
        );
    }

    #[test]
    fn biomedical_runtime_injects_domain_guidance() {
        let request = make_request("Summarize the trial results.", None);
        let runtime = make_runtime("biomedical", Some("Prefer CONSORT-aligned critique."));

        let instructions =
            build_agent_instructions_with_work_state(&request, None, Some(&runtime), None);

        assert!(instructions.contains("[Biomedical domain guardrails]"));
        assert!(instructions.contains("[Custom domain instructions]"));
        assert!(instructions.contains("CONSORT-aligned critique"));
    }

    #[test]
    fn language_policy_defaults_to_english_and_switches_to_chinese_on_explicit_request() {
        let default_request = make_request("Summarize the attached evidence.", None);
        let default_instructions =
            build_agent_instructions_with_work_state(&default_request, None, None, None);
        assert!(default_instructions.contains("respond in English by default"));

        let zh_request = make_request("请用中文总结这篇文章。", None);
        let zh_instructions =
            build_agent_instructions_with_work_state(&zh_request, None, None, None);
        assert!(zh_instructions.contains("respond in Chinese for this turn"));
    }

    #[test]
    fn openai_provider_instructions_include_structured_output_hint() {
        let request = make_request("Summarize key findings.", None);
        let mut runtime = make_runtime("general", None);
        runtime.provider = "openai".to_string();

        let instructions =
            build_agent_instructions_with_work_state(&request, None, Some(&runtime), None);

        assert!(instructions.contains("[Provider operating note]"));
        assert!(instructions.contains("strict JSON"));
    }

    #[test]
    fn minimax_provider_instructions_enforce_deeper_analysis() {
        let request = make_request(
            "Please compare the evidence quality across these papers.",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::LiteratureReview,
                sampling_profile: AgentSamplingProfile::AnalysisDeep,
                ..AgentTurnProfile::default()
            }),
        );
        let runtime = make_runtime("general", None);

        let instructions =
            build_agent_instructions_with_work_state(&request, None, Some(&runtime), None);

        assert!(instructions.contains("[Provider operating note]"));
        assert!(instructions.contains("Maintain strong reasoning depth"));
        assert!(instructions.contains("at least 3 evidence-backed points"));
    }

    #[test]
    fn selective_session_recall_includes_pending_state_and_avoids_echoing_identical_objective() {
        let request = make_request(
            "[Currently open file: main.tex]\n[Selection: @main.tex:10:1-10:20]\n[Selected text:\nfoo\n]\n\nrefine this paragraph",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::SelectionEdit,
                selection_scope: AgentSelectionScope::SelectedSpan,
                response_mode: AgentResponseMode::ReviewableChange,
                ..AgentTurnProfile::default()
            }),
        );
        let work_state = AgentSessionWorkState {
            current_objective: Some("refine this paragraph".to_string()),
            recent_objective: Some("tighten the related work section".to_string()),
            current_target: Some("main.tex".to_string()),
            last_tool_activity: Some("Completed apply_text_patch on main.tex".to_string()),
            pending_state: Some("review_ready".to_string()),
            pending_tool_name: Some("patch_file".to_string()),
            pending_target: Some("main.tex".to_string()),
            collected_references: Vec::new(),
            academic_workflow: Default::default(),
        };

        let instructions =
            build_agent_instructions_with_work_state(&request, Some(&work_state), None, None);

        assert!(instructions.contains("Pending state: review_ready via patch_file on main.tex"));
        assert!(instructions.contains("Working target: main.tex"));
        assert!(instructions.contains("Recent objective: tighten the related work section"));
        assert!(!instructions.contains("Active objective: refine this paragraph"));
    }

    #[test]
    fn binary_attachment_analysis_includes_document_strategy_block() {
        let request = make_request(
            "[Attached resource: @research.pdf (pdf)]\n[Resource path: attachments/research.pdf]\n[Attached excerpt:\nIntroduction and methods\n]\n\nSummarize the key findings and recommendations from this paper.",
            Some(AgentTurnProfile {
                task_kind: AgentTaskKind::Analysis,
                ..AgentTurnProfile::default()
            }),
        );

        let instructions = build_agent_instructions_with_work_state(&request, None, None, None);

        assert!(instructions.contains("[Document analysis strategy]"));
        assert!(instructions.contains("inspect_resource"));
        assert!(instructions.contains("read_document"));
        assert!(instructions.contains("search_document_text"));
        assert!(instructions.contains("Synthesize all found evidence into a comprehensive answer"));
    }

    #[test]
    fn summarize_objective_skips_context_markers_and_truncates_long_text() {
        let summary = summarize_objective(
            "[Currently open file: main.tex]\n[Selection: @main.tex:1:1-1:5]\nThis is the real request line that should be summarized even when it is intentionally made very long so the helper has to truncate it safely.",
        )
        .expect("summary");

        assert!(summary.starts_with("This is the real request line"));
        assert!(summary.ends_with("..."));
        assert!(summary.len() <= 123);
    }

    #[test]
    fn max_rounds_for_task_matches_turn_kind_budget_policy() {
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::SuggestionOnly,
                ..AgentTurnProfile::default()
            }),
            2
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::SelectionEdit,
                ..AgentTurnProfile::default()
            }),
            12
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::FileEdit,
                ..AgentTurnProfile::default()
            }),
            25
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::LiteratureReview,
                ..AgentTurnProfile::default()
            }),
            30
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::PaperDrafting,
                ..AgentTurnProfile::default()
            }),
            25
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::PeerReview,
                ..AgentTurnProfile::default()
            }),
            20
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::Analysis,
                ..AgentTurnProfile::default()
            }),
            25
        );
        assert_eq!(
            max_rounds_for_task(&AgentTurnProfile {
                task_kind: AgentTaskKind::General,
                ..AgentTurnProfile::default()
            }),
            25
        );
    }
}
