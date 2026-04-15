pub mod chat_completions;
pub mod openai;

#[derive(Debug, Clone)]
pub struct AgentTurnOutcome {
    pub response_id: Option<String>,
    pub messages: Vec<serde_json::Value>,
    pub suspended: bool,
}
