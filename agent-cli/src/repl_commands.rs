#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    Config,
    Help,
    Unknown(String),
    None,
}

pub fn parse_repl_command(input: &str) -> ReplCommand {
    match input.trim() {
        "/config" => ReplCommand::Config,
        "/help" => ReplCommand::Help,
        cmd if cmd.starts_with('/') => ReplCommand::Unknown(cmd.to_string()),
        _ => ReplCommand::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_repl_command, ReplCommand};

    #[test]
    fn parses_config_command() {
        assert_eq!(parse_repl_command("/config"), ReplCommand::Config);
    }

    #[test]
    fn parses_help_command() {
        assert_eq!(parse_repl_command("/help"), ReplCommand::Help);
    }
}
