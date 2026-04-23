use super::types::{UiFocus, UiLine, UiLineKind, ViewUpdate};

pub struct TuiViewModel {
    pub session_id: String,
    pub lines: Vec<UiLine>,
    pub focus: UiFocus,
    pub selected_line: usize,
    pub input_buffer: String,
    pub input_history: Vec<String>,
    pub history_cursor: Option<usize>,
    pub waiting_for_approval: bool,
}

impl TuiViewModel {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            lines: Vec::new(),
            focus: UiFocus::Input,
            selected_line: 0,
            input_buffer: String::new(),
            input_history: Vec::new(),
            history_cursor: None,
            waiting_for_approval: false,
        }
    }

    pub fn push_user_prompt(&mut self, prompt: String) {
        self.input_history.push(prompt.clone());
        self.history_cursor = None;
        self.lines.push(UiLine {
            kind: UiLineKind::User,
            prefix: "›".to_string(),
            text: prompt,
            details: Vec::new(),
            expanded: false,
        });
        self.selected_line = self.lines.len().saturating_sub(1);
    }

    pub fn apply_update(&mut self, update: ViewUpdate) {
        match update {
            ViewUpdate::AssistantDelta(delta) => self.lines.push(UiLine {
                kind: UiLineKind::Assistant,
                prefix: "●".to_string(),
                text: delta,
                details: Vec::new(),
                expanded: false,
            }),
            ViewUpdate::Semantic { text, detail } => self.lines.push(UiLine {
                kind: UiLineKind::Semantic,
                prefix: "└".to_string(),
                text,
                details: vec![detail],
                expanded: false,
            }),
            ViewUpdate::WaitingApproval(hint) => {
                self.waiting_for_approval = true;
                self.lines.push(UiLine {
                    kind: UiLineKind::Semantic,
                    prefix: "└".to_string(),
                    text: "Waiting for approval".to_string(),
                    details: vec![hint],
                    expanded: false,
                });
            }
            ViewUpdate::TurnOutcome(outcome) => {
                self.waiting_for_approval = outcome == "suspended";
            }
            ViewUpdate::Error(message) => self.lines.push(UiLine {
                kind: UiLineKind::Semantic,
                prefix: "└".to_string(),
                text: format!("Error: {}", message),
                details: Vec::new(),
                expanded: false,
            }),
        }
        self.selected_line = self.lines.len().saturating_sub(1);
    }

    pub fn toggle_detail(&mut self) {
        if self.focus != UiFocus::Timeline {
            return;
        }
        if let Some(line) = self.lines.get_mut(self.selected_line) {
            if line.kind == UiLineKind::Semantic && !line.details.is_empty() {
                line.expanded = !line.expanded;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_user_assistant_semantic_lines() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.push_user_prompt("read one file".to_string());
        vm.apply_update(ViewUpdate::AssistantDelta("I will inspect now.".to_string()));
        vm.apply_update(ViewUpdate::Semantic {
            text: "Read 1 file".to_string(),
            detail: "tool=read_file path=src/main.rs".to_string(),
        });

        assert_eq!(vm.lines.len(), 3);
        assert_eq!(vm.lines[0].prefix, "›");
        assert_eq!(vm.lines[1].prefix, "●");
        assert_eq!(vm.lines[2].prefix, "└");
    }

    #[test]
    fn toggles_detail_only_for_semantic_line() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.apply_update(ViewUpdate::Semantic {
            text: "Waiting for approval".to_string(),
            detail: "run /approve shell once".to_string(),
        });
        vm.focus = UiFocus::Timeline;
        vm.selected_line = 0;
        vm.toggle_detail();
        assert!(vm.lines[0].expanded);
    }
}
