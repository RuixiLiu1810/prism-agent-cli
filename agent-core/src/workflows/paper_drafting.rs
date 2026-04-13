use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperDraftingStage {
    OutlineConfirmation,
    SectionDrafting,
    ConsistencyCheck,
    RevisionPass,
    FinalPackaging,
    Completed,
}

impl PaperDraftingStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OutlineConfirmation => "outline_confirmation",
            Self::SectionDrafting => "section_drafting",
            Self::ConsistencyCheck => "consistency_check",
            Self::RevisionPass => "revision_pass",
            Self::FinalPackaging => "final_packaging",
            Self::Completed => "completed",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::OutlineConfirmation => "Outline Confirmation",
            Self::SectionDrafting => "Section Drafting",
            Self::ConsistencyCheck => "Consistency Check",
            Self::RevisionPass => "Revision Pass",
            Self::FinalPackaging => "Final Packaging",
            Self::Completed => "Completed",
        }
    }

    pub fn instruction(&self) -> &'static str {
        match self {
            Self::OutlineConfirmation => {
                "Confirm or refine the manuscript outline and section plan before drafting full prose."
            }
            Self::SectionDrafting => {
                "Draft or revise section-level manuscript prose according to the approved outline."
            }
            Self::ConsistencyCheck => {
                "Run terminology/abbreviation/numbering consistency checks and report concrete fixes."
            }
            Self::RevisionPass => {
                "Apply revision-focused improvements based on consistency findings and user feedback."
            }
            Self::FinalPackaging => {
                "Prepare final delivery output with concise completion notes and remaining risks."
            }
            Self::Completed => "Workflow completed.",
        }
    }

    pub fn next_stage(&self) -> Option<Self> {
        match self {
            Self::OutlineConfirmation => Some(Self::SectionDrafting),
            Self::SectionDrafting => Some(Self::ConsistencyCheck),
            Self::ConsistencyCheck => Some(Self::RevisionPass),
            Self::RevisionPass => Some(Self::FinalPackaging),
            Self::FinalPackaging | Self::Completed => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed)
    }
}
