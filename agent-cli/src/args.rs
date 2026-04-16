use clap::Parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    SingleTurn,
    Repl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Jsonl,
}

pub fn parse_output_mode(raw: &str) -> Result<OutputMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "human" => Ok(OutputMode::Human),
        "jsonl" => Ok(OutputMode::Jsonl),
        other => Err(format!(
            "Unsupported output mode '{}'. Use 'human' or 'jsonl'.",
            other
        )),
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "agent-runtime", version)]
pub struct Args {
    #[arg(long, env = "AGENT_API_KEY")]
    pub api_key: String,

    #[arg(long, env = "AGENT_PROVIDER", default_value = "minimax")]
    pub provider: String,

    #[arg(long, env = "AGENT_MODEL")]
    pub model: String,

    #[arg(long, env = "AGENT_BASE_URL")]
    pub base_url: Option<String>,

    #[arg(long)]
    pub project_path: String,

    #[arg(long)]
    pub prompt: Option<String>,

    #[arg(long, default_value = "cli-tab")]
    pub tab_id: String,

    #[arg(long, default_value = "human")]
    pub output: String,
}

impl Args {
    pub fn run_mode(&self) -> RunMode {
        if self.prompt.as_deref().is_some_and(|p| !p.trim().is_empty()) {
            RunMode::SingleTurn
        } else {
            RunMode::Repl
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_output_mode, Args, OutputMode, RunMode};
    use clap::Parser;

    #[test]
    fn detects_single_turn_when_prompt_is_present() {
        let args = Args::parse_from([
            "agent-runtime",
            "--api-key",
            "k",
            "--project-path",
            ".",
            "--model",
            "MiniMax-M1",
            "--prompt",
            "hello",
        ]);
        assert_eq!(args.run_mode(), RunMode::SingleTurn);
    }

    #[test]
    fn detects_repl_when_prompt_is_absent() {
        let args = Args::parse_from([
            "agent-runtime",
            "--api-key",
            "k",
            "--project-path",
            ".",
            "--model",
            "MiniMax-M1",
        ]);
        assert_eq!(args.run_mode(), RunMode::Repl);
    }

    #[test]
    fn parses_output_mode_human_and_jsonl() {
        assert_eq!(parse_output_mode("human").unwrap(), OutputMode::Human);
        assert_eq!(parse_output_mode("jsonl").unwrap(), OutputMode::Jsonl);
    }
}
