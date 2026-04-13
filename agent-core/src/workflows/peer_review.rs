use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeerReviewStage {
    ScopeAndCriteria,
    SectionReview,
    StatisticsReview,
    ReportAndRevisionPlan,
    Completed,
}

impl PeerReviewStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ScopeAndCriteria => "scope_and_criteria",
            Self::SectionReview => "section_review",
            Self::StatisticsReview => "statistics_review",
            Self::ReportAndRevisionPlan => "report_and_revision_plan",
            Self::Completed => "completed",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::ScopeAndCriteria => "Scope & Criteria",
            Self::SectionReview => "Section Review",
            Self::StatisticsReview => "Statistics Review",
            Self::ReportAndRevisionPlan => "Report & Revision Plan",
            Self::Completed => "Completed",
        }
    }

    pub fn instruction(&self) -> &'static str {
        match self {
            Self::ScopeAndCriteria => {
                "Define manuscript scope, review criteria, and severity policy before issuing findings."
            }
            Self::SectionReview => {
                "Run section-by-section review and capture actionable findings with evidence and severity labels."
            }
            Self::StatisticsReview => {
                "Audit reported statistics and flag unsupported claims, missing uncertainty, or unclear methods."
            }
            Self::ReportAndRevisionPlan => {
                "Produce a structured peer-review report and a concrete revision plan or response-letter outline."
            }
            Self::Completed => "Workflow completed.",
        }
    }

    pub fn next_stage(&self) -> Option<Self> {
        match self {
            Self::ScopeAndCriteria => Some(Self::SectionReview),
            Self::SectionReview => Some(Self::StatisticsReview),
            Self::StatisticsReview => Some(Self::ReportAndRevisionPlan),
            Self::ReportAndRevisionPlan | Self::Completed => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed)
    }
}
