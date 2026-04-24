use super::theme::{Role, Theme};
#[cfg(test)]
use super::types::{UiLine, UiLineKind};

#[cfg(test)]
pub fn append_assistant_delta(lines: &mut Vec<UiLine>, delta: String) {
    if delta.trim().is_empty() {
        return;
    }
    if let Some(last) = lines.last_mut() {
        if last.kind == UiLineKind::Assistant {
            last.text.push_str(&delta);
            return;
        }
    }

    lines.push(UiLine {
        kind: UiLineKind::Assistant,
        prefix: "●".to_string(),
        text: delta,
        details: Vec::new(),
        expanded: false,
    });
}

#[cfg(test)]
pub fn append_semantic(lines: &mut Vec<UiLine>, text: String, details: Vec<String>) {
    lines.push(UiLine {
        kind: UiLineKind::Semantic,
        prefix: "└".to_string(),
        text,
        details,
        expanded: false,
    });
}

fn wrap_with_prefix(prefix: &str, text: &str, width: usize) -> Vec<String> {
    let prefix_len = prefix.chars().count();
    let max_width = width.max(prefix_len + 1);
    let mut lines = Vec::new();
    let mut current = prefix.to_string();
    let mut current_len = prefix_len;

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let needs_space = current_len > prefix_len;
        let add_len = if needs_space { 1 + word_len } else { word_len };

        if current_len + add_len > max_width && current_len > prefix_len {
            lines.push(current);
            current = format!("{}{}", " ".repeat(prefix_len), word);
            current_len = prefix_len + word_len;
            continue;
        }

        if needs_space {
            current.push(' ');
        }
        current.push_str(word);
        current_len += add_len;
    }

    lines.push(current);
    lines
}

pub fn render_user_command_rows(theme: &Theme, text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    wrap_with_prefix("› ", text, width)
        .into_iter()
        .map(|mut line| {
            let line_len = line.chars().count();
            if line_len < width {
                line.push_str(&" ".repeat(width - line_len));
            }
            theme.paint(Role::CommandRowBg, line)
        })
        .collect()
}

#[cfg(test)]
pub fn render_assistant_block(marker: &str, text: &str, width: usize) -> Vec<String> {
    wrap_with_prefix(&format!("{} ", marker), text, width)
}

#[cfg(test)]
mod tests {
    use super::{append_assistant_delta, append_semantic};
    use crate::tui::types::{UiLine, UiLineKind};

    #[test]
    fn merges_adjacent_assistant_delta_chunks() {
        let mut lines = Vec::<UiLine>::new();
        append_assistant_delta(&mut lines, "Hello".to_string());
        append_assistant_delta(&mut lines, " world".to_string());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, UiLineKind::Assistant);
        assert_eq!(lines[0].text, "Hello world");
    }

    #[test]
    fn appends_semantic_line_with_expandable_detail() {
        let mut lines = Vec::<UiLine>::new();
        append_semantic(
            &mut lines,
            "Read src/main.rs".to_string(),
            vec!["tool=read_file".to_string()],
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, UiLineKind::Semantic);
        assert_eq!(lines[0].details.len(), 1);
    }

    #[test]
    fn renders_user_command_row_with_prefix_and_background() {
        let theme = crate::tui::theme::Theme { enable_color: true };
        let rows = super::render_user_command_rows(&theme, "who are you", 40);
        assert!(rows[0].contains("› who are you"));
        assert!(rows[0].contains("\x1b[48;"));
    }

    #[test]
    fn renders_user_command_row_full_width_when_no_color() {
        let theme = crate::tui::theme::Theme {
            enable_color: false,
        };
        let rows = super::render_user_command_rows(&theme, "hi", 12);
        assert_eq!(rows[0].chars().count(), 12);
        assert!(rows[0].starts_with("› hi"));
    }

    #[test]
    fn assistant_block_wraps_with_hanging_indent() {
        let lines = super::render_assistant_block(
            "●",
            "I can help with coding debugging and architecture decisions",
            28,
        );
        assert!(lines[0].starts_with("● "));
        assert!(lines[1].starts_with("  "));
    }
}
