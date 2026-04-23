use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Command,
    SingleTurn,
    Repl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Off,
    Safe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Tui,
    Classic,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum ConfigSubcommand {
    Init,
    Edit,
    Show,
    Path,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Command {
    Config {
        #[command(subcommand)]
        action: ConfigSubcommand,
    },
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

pub fn parse_tool_mode(raw: &str) -> Result<ToolMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" => Ok(ToolMode::Off),
        "safe" => Ok(ToolMode::Safe),
        other => Err(format!(
            "Unsupported tool mode '{}'. Use 'off' or 'safe'.",
            other
        )),
    }
}

pub fn parse_ui_mode(raw: &str) -> Result<UiMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "tui" => Ok(UiMode::Tui),
        "classic" => Ok(UiMode::Classic),
        other => Err(format!(
            "Unsupported ui mode '{}'. Use 'tui' or 'classic'.",
            other
        )),
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "agent-runtime", version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long)]
    pub api_key: Option<String>,

    #[arg(long)]
    pub provider: Option<String>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub base_url: Option<String>,

    #[arg(long, default_value = ".")]
    pub project_path: String,

    #[arg(long)]
    pub prompt: Option<String>,

    #[arg(long, default_value = "cli-tab")]
    pub tab_id: String,

    #[arg(long)]
    pub output: Option<String>,

    #[arg(long, env = "AGENT_TOOL_MODE")]
    pub tool_mode: Option<String>,

    #[arg(long, env = "AGENT_UI_MODE")]
    pub ui_mode: Option<String>,
}

impl Args {
    pub fn run_mode(&self) -> RunMode {
        if self.command.is_some() {
            RunMode::Command
        } else if self.prompt.as_deref().is_some_and(|p| !p.trim().is_empty()) {
            RunMode::SingleTurn
        } else {
            RunMode::Repl
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_output_mode, parse_tool_mode, parse_ui_mode, Args, Command, ConfigSubcommand,
        OutputMode, RunMode, ToolMode, UiMode,
    };
    use clap::Parser;

    #[test]
    fn detects_single_turn_when_prompt_is_present() {
        let args = Args::parse_from([
            "agent-runtime",
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
            "--project-path",
            ".",
            "--model",
            "MiniMax-M1",
        ]);
        assert_eq!(args.run_mode(), RunMode::Repl);
    }

    #[test]
    fn detects_config_subcommand_mode() {
        let args = Args::parse_from(["agent-runtime", "config", "path"]);
        assert_eq!(args.run_mode(), RunMode::Command);
    }

    #[test]
    fn parses_config_edit_subcommand() {
        let args = Args::parse_from(["agent-runtime", "config", "edit"]);
        assert!(matches!(
            args.command,
            Some(Command::Config {
                action: ConfigSubcommand::Edit
            })
        ));
    }

    #[test]
    fn parses_output_mode_human_and_jsonl() {
        assert_eq!(
            parse_output_mode("human").unwrap_or_else(|e| panic!("parse: {e}")),
            OutputMode::Human
        );
        assert_eq!(
            parse_output_mode("jsonl").unwrap_or_else(|e| panic!("parse: {e}")),
            OutputMode::Jsonl
        );
    }

    #[test]
    fn parses_tool_mode_safe_and_off() {
        let args = Args::parse_from(["agent-runtime", "--tool-mode", "safe"]);
        assert_eq!(args.tool_mode.as_deref(), Some("safe"));

        let args = Args::parse_from(["agent-runtime", "--tool-mode", "off"]);
        assert_eq!(args.tool_mode.as_deref(), Some("off"));
    }

    #[test]
    fn parse_tool_mode_rejects_unknown_value() {
        let err = parse_tool_mode("danger").err();
        assert!(err.is_some());
    }

    #[test]
    fn parse_tool_mode_parses_safe_and_off() {
        assert_eq!(
            parse_tool_mode("safe").unwrap_or_else(|e| panic!("parse: {e}")),
            ToolMode::Safe
        );
        assert_eq!(
            parse_tool_mode("off").unwrap_or_else(|e| panic!("parse: {e}")),
            ToolMode::Off
        );
    }

    #[test]
    fn parses_ui_mode_tui_and_classic() {
        assert_eq!(
            parse_ui_mode("tui").unwrap_or_else(|e| panic!("parse: {e}")),
            UiMode::Tui
        );
        assert_eq!(
            parse_ui_mode("classic").unwrap_or_else(|e| panic!("parse: {e}")),
            UiMode::Classic
        );
    }

    #[test]
    fn rejects_unknown_ui_mode() {
        let err = parse_ui_mode("fancy").err();
        assert!(err.is_some());
    }
}
