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
}
