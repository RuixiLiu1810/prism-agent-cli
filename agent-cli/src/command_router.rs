const KNOWN_COMMANDS: &[&str] = &[
    "/help",
    "/commands",
    "/config",
    "/model",
    "/status",
    "/clear",
    "/approve",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    Help,
    Commands,
    Config,
    Status,
    Clear,
    ModelShow,
    ModelSet(String),
    ApproveShellOnce,
    ApproveShellSession,
    ApproveShellDeny,
    Unknown {
        raw: String,
        suggestion: Option<&'static str>,
    },
    None,
}

pub fn parse_repl_command(input: &str) -> ReplCommand {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return ReplCommand::None;
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or(trimmed);

    match command {
        "/help" => ReplCommand::Help,
        "/commands" => ReplCommand::Commands,
        "/config" => ReplCommand::Config,
        "/status" => ReplCommand::Status,
        "/clear" => ReplCommand::Clear,
        "/model" => {
            let model = parts.collect::<Vec<_>>().join(" ");
            if model.trim().is_empty() {
                ReplCommand::ModelShow
            } else {
                ReplCommand::ModelSet(model.trim().to_string())
            }
        }
        "/approve" => {
            let target = parts.next().unwrap_or_default();
            let mode = parts.next().unwrap_or_default();
            match (target, mode) {
                ("shell", "once") => ReplCommand::ApproveShellOnce,
                ("shell", "session") => ReplCommand::ApproveShellSession,
                ("shell", "deny") => ReplCommand::ApproveShellDeny,
                _ => ReplCommand::Unknown {
                    raw: trimmed.to_string(),
                    suggestion: Some("/approve shell once"),
                },
            }
        }
        other => ReplCommand::Unknown {
            raw: other.to_string(),
            suggestion: suggest_command(other),
        },
    }
}

fn suggest_command(raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    let mut best = None;

    for candidate in KNOWN_COMMANDS {
        let distance = levenshtein(raw, candidate);
        let threshold = raw.len().max(candidate.len()).min(4);
        if distance <= threshold {
            match best {
                Some((best_distance, _)) if best_distance <= distance => {}
                _ => best = Some((distance, *candidate)),
            }
        }
    }

    best.map(|(_, candidate)| candidate)
}

fn levenshtein(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();

    let mut previous: Vec<usize> = (0..=right_chars.len()).collect();
    let mut current = vec![0usize; right_chars.len() + 1];

    for (i, left_char) in left_chars.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right_chars.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            current[j + 1] = (current[j] + 1)
                .min(previous[j + 1] + 1)
                .min(previous[j] + cost);
        }
        previous.clone_from_slice(&current);
    }

    previous[right_chars.len()]
}

#[cfg(test)]
mod tests {
    use super::{parse_repl_command, ReplCommand};

    #[test]
    fn parses_supported_commands() {
        assert_eq!(parse_repl_command("/help"), ReplCommand::Help);
        assert_eq!(parse_repl_command("/commands"), ReplCommand::Commands);
        assert_eq!(parse_repl_command("/config"), ReplCommand::Config);
        assert_eq!(parse_repl_command("/status"), ReplCommand::Status);
        assert_eq!(parse_repl_command("/clear"), ReplCommand::Clear);
    }

    #[test]
    fn parses_model_show_and_set() {
        assert_eq!(parse_repl_command("/model"), ReplCommand::ModelShow);
        assert_eq!(
            parse_repl_command("/model MiniMax-M1"),
            ReplCommand::ModelSet("MiniMax-M1".to_string())
        );
    }

    #[test]
    fn parses_approve_shell_commands() {
        assert_eq!(
            parse_repl_command("/approve shell once"),
            ReplCommand::ApproveShellOnce
        );
        assert_eq!(
            parse_repl_command("/approve shell session"),
            ReplCommand::ApproveShellSession
        );
        assert_eq!(
            parse_repl_command("/approve shell deny"),
            ReplCommand::ApproveShellDeny
        );
    }

    #[test]
    fn returns_suggestion_for_unknown_commands() {
        assert_eq!(
            parse_repl_command("/commnads"),
            ReplCommand::Unknown {
                raw: "/commnads".to_string(),
                suggestion: Some("/commands"),
            }
        );
    }

    #[test]
    fn returns_none_for_non_commands() {
        assert_eq!(parse_repl_command("hello"), ReplCommand::None);
    }
}
