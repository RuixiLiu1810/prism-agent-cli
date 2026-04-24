#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Scrollable,
    Bottom,
    Overlay,
    Modal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotLine {
    pub slot: Slot,
    pub text: String,
}

impl SlotLine {
    pub fn new(slot: Slot, text: impl Into<String>) -> Self {
        Self {
            slot,
            text: text.into(),
        }
    }
}

fn clip_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    text.chars().take(width).collect()
}

pub fn render_slots(lines: &[SlotLine], width: Option<usize>) -> String {
    let mut out = String::new();
    let mut wrote_any = false;

    for slot in [Slot::Scrollable, Slot::Bottom, Slot::Overlay, Slot::Modal] {
        let mut slot_lines = lines.iter().filter(|line| line.slot == slot).peekable();
        if slot_lines.peek().is_none() {
            continue;
        }

        if wrote_any {
            out.push('\n');
        }

        for line in slot_lines {
            let rendered = match width {
                Some(width) => clip_to_width(&line.text, width),
                None => line.text.clone(),
            };
            out.push_str(&rendered);
            if !rendered.ends_with('\n') {
                out.push('\n');
            }
        }
        wrote_any = true;
    }

    out
}

pub fn render_header_block(
    product: &str,
    version: &str,
    model_line: &str,
    path: &str,
) -> Vec<String> {
    let logo_lines: Vec<&str> = Icons::project_logo().lines().collect();
    let status_lines = [
        format!("{} {}", product, version),
        model_line.to_string(),
        path.to_string(),
    ];
    let total_lines = logo_lines.len().max(status_lines.len());
    let logo_width = logo_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let left_col_width = logo_width + 2;

    let mut out = Vec::with_capacity(total_lines);
    for i in 0..total_lines {
        let left = logo_lines.get(i).copied().unwrap_or("");
        let right = status_lines.get(i).map(String::as_str).unwrap_or("");
        if right.is_empty() {
            out.push(left.to_string());
        } else {
            out.push(format!("{left:<left_col_width$}{right}"));
        }
    }
    out
}

pub fn render_notice_line(primary: &str, hint: &str) -> String {
    format!("{} · {}", primary, hint)
}

#[cfg(test)]
mod tests {
    use super::{render_slots, Slot, SlotLine};

    #[test]
    fn renders_by_slot_order_not_input_order() {
        let lines = vec![
            SlotLine::new(Slot::Overlay, "overlay"),
            SlotLine::new(Slot::Scrollable, "scrollable"),
            SlotLine::new(Slot::Modal, "modal"),
            SlotLine::new(Slot::Bottom, "bottom"),
        ];
        let out = render_slots(&lines, None);
        let expected = "scrollable\n\nbottom\n\noverlay\n\nmodal\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn clips_lines_for_narrow_width() {
        let lines = vec![SlotLine::new(Slot::Scrollable, "123456789")];
        let out = render_slots(&lines, Some(5));
        assert_eq!(out, "12345\n");
    }

    #[test]
    fn render_header_block_has_no_border_lines() {
        let lines = super::render_header_block(
            "Claude Prism",
            "v0.1.0",
            "MiniMax-M1 · safe mode",
            "~/Documents/Code/claude-prism",
        );
        assert!(!lines
            .iter()
            .any(|line| line.contains("===") || line.contains("---")));
        assert!(lines.iter().any(|line| line.contains("Claude Prism")));
    }

    #[test]
    fn render_notice_line_uses_plain_single_line_text() {
        let line = super::render_notice_line("Tool approvals enabled", "/commands for help");
        assert!(line.contains("Tool approvals enabled"));
        assert!(line.contains("/commands for help"));
        assert!(!line.contains('\n'));
    }
}
use super::icons::Icons;
