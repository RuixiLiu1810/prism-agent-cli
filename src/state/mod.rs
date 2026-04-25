use crate::commands::{parse_output_mode, parse_tool_mode, Args, OutputMode, ToolMode};

pub mod runtime_state;

pub use runtime_state::{PendingTurn, RuntimeState};

#[derive(Debug, Clone)]
pub struct BootstrapState {
    pub provider: String,
    pub model: String,
    pub project_path: String,
    pub output_mode: String,
    pub tool_mode: String,
}

impl BootstrapState {
    pub fn from_args(args: &Args) -> Self {
        let output_mode = args
            .output
            .as_deref()
            .and_then(|raw| parse_output_mode(raw).ok())
            .unwrap_or(OutputMode::Human);
        let tool_mode = args
            .tool_mode
            .as_deref()
            .and_then(|raw| parse_tool_mode(raw).ok())
            .unwrap_or(ToolMode::Safe);

        Self {
            provider: crate::runtime::resolved_provider(args),
            model: args
                .model
                .as_deref()
                .map_or_else(|| "default-model".to_string(), |v| v.trim().to_string()),
            project_path: args.project_path.clone(),
            output_mode: match output_mode {
                OutputMode::Human => "human".to_string(),
                OutputMode::Jsonl => "jsonl".to_string(),
            },
            tool_mode: crate::tools::mode_label(tool_mode).to_string(),
        }
    }
}
