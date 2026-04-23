#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiFocus {
    Input,
    Timeline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiLineKind {
    User,
    Assistant,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiLine {
    pub kind: UiLineKind,
    pub prefix: String,
    pub text: String,
    pub details: Vec<String>,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewUpdate {
    AssistantDelta(String),
    Semantic { text: String, detail: String },
    WaitingApproval(String),
    TurnOutcome(String),
    Error(String),
}
