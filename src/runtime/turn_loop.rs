#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    pub tab_id: String,
    pub suspended: bool,
    pub stage: String,
    pub message: String,
}

impl TurnOutcome {
    pub fn suspended(tab_id: &str, message: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            suspended: true,
            stage: "awaiting_approval".to_string(),
            message: message.into(),
        }
    }

    pub fn completed(tab_id: &str, message: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            suspended: false,
            stage: "completed".to_string(),
            message: message.into(),
        }
    }
}
