use super::types::{UiLine, UiLineKind};

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

pub fn append_semantic(lines: &mut Vec<UiLine>, text: String, details: Vec<String>) {
    lines.push(UiLine {
        kind: UiLineKind::Semantic,
        prefix: "└".to_string(),
        text,
        details,
        expanded: false,
    });
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
}
