use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

const RESOURCE_ARTIFACT_DIR: &str = ".claudeprism/agent-resources";
const MATCH_SNIPPET_CHARS: usize = 220;
const SUPPORTED_ARTIFACT_VERSION: u32 = 2;

const ENGLISH_STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "this",
    "that",
    "from",
    "have",
    "has",
    "had",
    "were",
    "was",
    "are",
    "what",
    "which",
    "when",
    "where",
    "does",
    "did",
    "into",
    "than",
    "then",
    "them",
    "they",
    "their",
    "there",
    "about",
    "mentioned",
    "article",
    "paper",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentArtifactSegment {
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentArtifact {
    pub version: u32,
    pub file_path: String,
    pub absolute_path: String,
    pub source_type: String,
    pub kind: String,
    pub extraction_status: String,
    pub excerpt: String,
    pub searchable_text: String,
    pub segments: Vec<DocumentArtifactSegment>,
    pub page_count: Option<usize>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentEvidenceMatch {
    pub label: String,
    pub snippet: String,
    pub score: usize,
}

fn normalize_path(value: &str) -> String {
    value.replace('\\', "/").trim_end_matches('/').to_string()
}

fn to_hex_key(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

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

pub fn artifact_path_for(project_root: &str, relative_path: &str) -> PathBuf {
    Path::new(project_root)
        .join(RESOURCE_ARTIFACT_DIR)
        .join(format!(
            "{}.json",
            to_hex_key(&normalize_path(relative_path))
        ))
}

pub async fn load_document_artifact(
    project_root: &str,
    relative_path: &str,
) -> Result<DocumentArtifact, String> {
    let artifact_path = artifact_path_for(project_root, relative_path);
    let content = fs::read_to_string(&artifact_path).await.map_err(|err| {
        format!(
            "No ingested document artifact is available for {}. Attach or re-attach the document so ClaudePrism can ingest it before using document tools. ({})",
            relative_path, err
        )
    })?;

    let artifact = serde_json::from_str::<DocumentArtifact>(&content).map_err(|err| {
        format!(
            "Failed to parse the ingested document artifact for {}: {}",
            relative_path, err
        )
    })?;

    if artifact.version < SUPPORTED_ARTIFACT_VERSION {
        return Err(format!(
            "The ingested document artifact for {} is outdated (v{}). Re-attach the document so ClaudePrism can re-ingest it.",
            relative_path, artifact.version
        ));
    }

    Ok(artifact)
}

fn collapse_for_snippet(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn make_snippet(text: &str, index: usize, needle_len: usize) -> String {
    let start = index.saturating_sub(MATCH_SNIPPET_CHARS / 2);
    let end = (index + needle_len + MATCH_SNIPPET_CHARS / 2).min(text.len());
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < text.len() { "..." } else { "" };
    format!(
        "{}{}{}",
        prefix,
        collapse_for_snippet(&text[start..end]),
        suffix
    )
}

fn derive_search_needles(query: &str) -> Vec<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut needles = Vec::new();
    let lower = trimmed.to_lowercase();
    let english_terms = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .map(str::trim)
        .filter(|term| term.len() >= 2 && !ENGLISH_STOP_WORDS.contains(term))
        .map(str::to_string)
        .collect::<Vec<_>>();

    if (2..=6).contains(&english_terms.len()) {
        needles.push(english_terms.join(" "));
    }
    needles.extend(english_terms);

    let mut cjk_buffer = String::new();
    for ch in trimmed.chars() {
        if ('\u{3400}'..='\u{9fff}').contains(&ch) {
            cjk_buffer.push(ch);
        } else if !cjk_buffer.is_empty() {
            if cjk_buffer.chars().count() >= 2 {
                needles.push(cjk_buffer.clone());
            }
            cjk_buffer.clear();
        }
    }
    if cjk_buffer.chars().count() >= 2 {
        needles.push(cjk_buffer);
    }

    needles.sort();
    needles.dedup();
    needles
}

pub fn find_relevant_document_matches(
    artifact: &DocumentArtifact,
    query: &str,
    limit: usize,
) -> Vec<DocumentEvidenceMatch> {
    let needles = derive_search_needles(query);
    if needles.is_empty() {
        return Vec::new();
    }

    let mut matches = artifact
        .segments
        .iter()
        .filter_map(|segment| {
            let haystack = segment.text.to_lowercase();
            let mut score = 0usize;
            let mut best_index = None;
            let mut best_needle_len = 0usize;

            for needle in &needles {
                let mut from = 0usize;
                let mut hits = 0usize;
                while let Some(found) = haystack[from..].find(needle) {
                    let absolute = from + found;
                    if best_index.is_none() {
                        best_index = Some(absolute);
                        best_needle_len = needle.len();
                    }
                    hits += 1;
                    from = absolute + needle.len();
                }

                if hits > 0 {
                    let weight = if needle.contains(' ')
                        || needle
                            .chars()
                            .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
                    {
                        6
                    } else {
                        2
                    };
                    score += hits * weight;
                }
            }

            best_index.map(|index| DocumentEvidenceMatch {
                label: segment.label.clone(),
                snippet: make_snippet(&segment.text, index, best_needle_len.max(1)),
                score,
            })
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| right.score.cmp(&left.score));
    matches.truncate(limit);
    matches
}

pub fn format_document_matches_preview(
    path: &str,
    source_type: &str,
    matches: &[DocumentEvidenceMatch],
    prefix: &str,
) -> String {
    if matches.is_empty() {
        return format!(
            "{} in {} ({}):\nNo relevant matches found.",
            prefix, path, source_type
        );
    }

    let mut lines = vec![format!("{} in {} ({}):", prefix, path, source_type)];
    for matched in matches {
        lines.push(format!("- {}: {}", matched.label, matched.snippet));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_path_for, find_relevant_document_matches, format_document_matches_preview,
        load_document_artifact, DocumentArtifact, DocumentArtifactSegment,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn make_artifact() -> DocumentArtifact {
        DocumentArtifact {
            version: 2,
            file_path: "attachments/paper.pdf".to_string(),
            absolute_path: "/tmp/project/attachments/paper.pdf".to_string(),
            source_type: "pdf".to_string(),
            kind: "pdf_document".to_string(),
            extraction_status: "ready".to_string(),
            excerpt: "excerpt".to_string(),
            searchable_text: "hydrophobic surface treatment".to_string(),
            segments: vec![
                DocumentArtifactSegment {
                    label: "Page 2".to_string(),
                    text: "Contact angle measurements show hydrophobic surface treatment on TiO2."
                        .to_string(),
                },
                DocumentArtifactSegment {
                    label: "Page 4".to_string(),
                    text: "Polydopamine coating improves adhesion but does not mention hydrophobic experiments."
                        .to_string(),
                },
            ],
            page_count: Some(6),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn artifact_path_uses_project_local_resource_dir() {
        let artifact_path = artifact_path_for("/tmp/project", "attachments/paper.pdf");
        let text = artifact_path.to_string_lossy();
        assert!(text.contains(".claudeprism/agent-resources"));
        assert!(text.ends_with(".json"));
    }

    #[test]
    fn document_match_search_finds_high_signal_segments() {
        let matches = find_relevant_document_matches(
            &make_artifact(),
            "哪篇文章提到疏水性实验 hydrophobic",
            3,
        );
        assert!(!matches.is_empty());
        assert_eq!(matches[0].label, "Page 2");
        assert!(matches[0].snippet.to_lowercase().contains("hydrophobic"));
    }

    #[test]
    fn format_document_matches_preview_is_readable() {
        let matches = find_relevant_document_matches(&make_artifact(), "hydrophobic", 2);
        let preview = format_document_matches_preview(
            "attachments/paper.pdf",
            "pdf",
            &matches,
            "Supporting evidence",
        );
        assert!(preview.contains("Supporting evidence"));
        assert!(preview.contains("attachments/paper.pdf"));
        assert!(preview.contains("Page 2"));
    }

    #[tokio::test]
    async fn rejects_outdated_artifact_versions() {
        let temp = tempdir().unwrap();
        let project_root = temp.path().to_string_lossy().to_string();
        let artifact_path = artifact_path_for(&project_root, "attachments/legacy.pdf");
        tokio::fs::create_dir_all(artifact_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &artifact_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "filePath": "attachments/legacy.pdf",
                "absolutePath": temp.path().join("attachments/legacy.pdf").to_string_lossy(),
                "sourceType": "pdf",
                "kind": "pdf_document",
                "extractionStatus": "ready",
                "excerpt": "legacy excerpt",
                "searchableText": "legacy text",
                "segments": [],
                "pageCount": 1,
                "metadata": {}
            }))
            .unwrap(),
        )
        .await
        .unwrap();

        let err = load_document_artifact(&project_root, "attachments/legacy.pdf")
            .await
            .unwrap_err();
        assert!(err.contains("outdated"));
    }
}
