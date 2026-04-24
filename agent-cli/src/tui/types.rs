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
    System,
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
    Semantic { text: String, details: Vec<String> },
    WaitingApproval(String),
    TurnOutcome(String),
    Error(String),
}
