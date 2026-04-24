use crate::command_router;

pub fn render_command_suggestions(input: &str, limit: usize) -> Option<String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let suggestions = command_router::suggest_commands(trimmed, limit);
    if suggestions.is_empty() {
        return None;
    }

    Some(format!("Suggestions: {}", suggestions.join("  ")))
}

#[cfg(test)]
mod tests {
    use super::render_command_suggestions;

    #[test]
    fn renders_suggestions_for_slash_prefix() {
        let rendered = render_command_suggestions("/ap", 3);
        assert_eq!(rendered.as_deref(), Some("Suggestions: /approve"));
    }

    #[test]
    fn returns_none_for_non_command_input() {
        assert!(render_command_suggestions("hello", 3).is_none());
    }
}
