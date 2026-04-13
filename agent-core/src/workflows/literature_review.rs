use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiteratureReviewStage {
    PicoScoping,
    SearchAndScreen,
    PaperAnalysis,
    EvidenceSynthesis,
    Completed,
}

impl LiteratureReviewStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PicoScoping => "pico_scoping",
            Self::SearchAndScreen => "search_and_screen",
            Self::PaperAnalysis => "paper_analysis",
            Self::EvidenceSynthesis => "evidence_synthesis",
            Self::Completed => "completed",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::PicoScoping => "PICO Scoping",
            Self::SearchAndScreen => "Search & Screening",
            Self::PaperAnalysis => "Paper Analysis",
            Self::EvidenceSynthesis => "Evidence Synthesis",
            Self::Completed => "Completed",
        }
    }

    pub fn instruction(&self) -> &'static str {
        match self {
            Self::PicoScoping => {
                "Clarify the literature objective with PICO-style framing and explicit inclusion/exclusion criteria."
            }
            Self::SearchAndScreen => {
                "Search literature with multi-provider coverage and screen candidates for relevance and quality."
            }
            Self::PaperAnalysis => {
                "Analyze selected papers and extract structured objective, methods, findings, and limitations."
            }
            Self::EvidenceSynthesis => {
                "Synthesize evidence into coherent themes with source-linked support and uncertainty notes."
            }
            Self::Completed => "Workflow completed.",
        }
    }

    pub fn next_stage(&self) -> Option<Self> {
        match self {
            Self::PicoScoping => Some(Self::SearchAndScreen),
            Self::SearchAndScreen => Some(Self::PaperAnalysis),
            Self::PaperAnalysis => Some(Self::EvidenceSynthesis),
            Self::EvidenceSynthesis | Self::Completed => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed)
    }
}
