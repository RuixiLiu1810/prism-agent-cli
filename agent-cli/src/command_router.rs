pub(crate) const KNOWN_COMMANDS: &[&str] = &[
    "/help",
    "/commands",
    "/config",
    "/model",
    "/permissions",
    "/status",
    "/clear",
    "/approve",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCommand {
    Show,
    ShellOnce,
    ShellSession,
    ShellDeny,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    Help,
    Commands,
    Config,
    Status,
    Clear,
    ModelShow,
    ModelSet(String),
    Permissions(PermissionCommand),
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
        "/permissions" => parse_permissions_command(&mut parts, trimmed),
        "/approve" => {
            let target = parts.next().unwrap_or_default();
            let mode = parts.next().unwrap_or_default();
            match (target, mode) {
                ("shell", "once") => ReplCommand::Permissions(PermissionCommand::ShellOnce),
                ("shell", "session") => ReplCommand::Permissions(PermissionCommand::ShellSession),
                ("shell", "deny") => ReplCommand::Permissions(PermissionCommand::ShellDeny),
                _ => ReplCommand::Unknown {
                    raw: trimmed.to_string(),
                    suggestion: Some("/permissions shell once"),
                },
            }
        }
        other => ReplCommand::Unknown {
            raw: other.to_string(),
            suggestion: suggest_command(other),
        },
    }
}

fn parse_permissions_command<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    raw: &str,
) -> ReplCommand {
    match parts.next() {
        None | Some("show") => ReplCommand::Permissions(PermissionCommand::Show),
        Some("clear") => ReplCommand::Permissions(PermissionCommand::Clear),
        Some("shell") => match parts.next() {
            Some("once") => ReplCommand::Permissions(PermissionCommand::ShellOnce),
            Some("session") => ReplCommand::Permissions(PermissionCommand::ShellSession),
            Some("deny") => ReplCommand::Permissions(PermissionCommand::ShellDeny),
            _ => ReplCommand::Unknown {
                raw: raw.to_string(),
                suggestion: Some("/permissions shell once"),
            },
        },
        _ => ReplCommand::Unknown {
            raw: raw.to_string(),
            suggestion: Some("/permissions"),
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

pub fn suggest_commands(prefix: &str, limit: usize) -> Vec<&'static str> {
    if limit == 0 {
        return Vec::new();
    }

    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return KNOWN_COMMANDS.iter().copied().take(limit).collect();
    }

    let mut starts_with: Vec<&'static str> = KNOWN_COMMANDS
        .iter()
        .copied()
        .filter(|command| command.starts_with(trimmed))
        .collect();
    starts_with.sort_unstable();

    if !starts_with.is_empty() {
        starts_with.truncate(limit);
        return starts_with;
    }

    let mut fuzzy = KNOWN_COMMANDS
        .iter()
        .copied()
        .filter(|candidate| !starts_with.contains(candidate))
        .filter_map(|candidate| {
            let distance = levenshtein(trimmed, candidate);
            let threshold = trimmed.len().max(candidate.len()).min(4);
            if distance <= threshold {
                Some((distance, candidate))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    fuzzy.sort_unstable_by_key(|(distance, candidate)| (*distance, *candidate));

    for (_, candidate) in fuzzy {
        starts_with.push(candidate);
        if starts_with.len() >= limit {
            break;
        }
    }
    starts_with
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
    use super::{parse_repl_command, PermissionCommand, ReplCommand};

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
            ReplCommand::Permissions(PermissionCommand::ShellOnce)
        );
        assert_eq!(
            parse_repl_command("/approve shell session"),
            ReplCommand::Permissions(PermissionCommand::ShellSession)
        );
        assert_eq!(
            parse_repl_command("/approve shell deny"),
            ReplCommand::Permissions(PermissionCommand::ShellDeny)
        );
    }

    #[test]
    fn parses_permissions_commands() {
        assert_eq!(
            parse_repl_command("/permissions"),
            ReplCommand::Permissions(PermissionCommand::Show)
        );
        assert_eq!(
            parse_repl_command("/permissions show"),
            ReplCommand::Permissions(PermissionCommand::Show)
        );
        assert_eq!(
            parse_repl_command("/permissions clear"),
            ReplCommand::Permissions(PermissionCommand::Clear)
        );
        assert_eq!(
            parse_repl_command("/permissions shell session"),
            ReplCommand::Permissions(PermissionCommand::ShellSession)
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

    #[test]
    fn suggests_prefix_candidates_for_commands() {
        let suggestions = super::suggest_commands("/ap", 3);
        assert_eq!(suggestions, vec!["/approve"]);
    }
}
