use super::history_search::HistorySearch;
use super::input_buffer::InputBuffer;
use super::transcript::{append_assistant_delta, append_semantic};
use super::types::{UiFocus, UiLine, UiLineKind, ViewUpdate};

pub struct TuiViewModel {
    pub session_id: String,
    pub lines: Vec<UiLine>,
    pub focus: UiFocus,
    pub selected_line: usize,
    pub input: InputBuffer,
    pub history: HistorySearch,
    pub waiting_for_approval: bool,
}

impl TuiViewModel {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            lines: Vec::new(),
            focus: UiFocus::Input,
            selected_line: 0,
            input: InputBuffer::default(),
            history: HistorySearch::default(),
            waiting_for_approval: false,
        }
    }

    pub fn push_user_prompt(&mut self, prompt: String) {
        self.history.record(prompt.clone());
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
            ViewUpdate::AssistantDelta(delta) => append_assistant_delta(&mut self.lines, delta),
            ViewUpdate::Semantic { text, details } => {
                append_semantic(&mut self.lines, text, details)
            }
            ViewUpdate::WaitingApproval(hint) => {
                self.waiting_for_approval = true;
                append_semantic(
                    &mut self.lines,
                    "Waiting for approval".to_string(),
                    vec![hint],
                );
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

    pub fn input_buffer(&self) -> &str {
        self.input.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_user_assistant_semantic_lines() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.push_user_prompt("read one file".to_string());
        vm.apply_update(ViewUpdate::AssistantDelta(
            "I will inspect now.".to_string(),
        ));
        vm.apply_update(ViewUpdate::AssistantDelta(" by listing files.".to_string()));
        vm.apply_update(ViewUpdate::Semantic {
            text: "Read 1 file".to_string(),
            details: vec!["tool=read_file path=src/main.rs".to_string()],
        });

        assert_eq!(vm.lines.len(), 3);
        assert_eq!(vm.lines[0].prefix, "›");
        assert_eq!(vm.lines[1].prefix, "●");
        assert_eq!(vm.lines[1].text, "I will inspect now. by listing files.");
        assert_eq!(vm.lines[2].prefix, "└");
    }

    #[test]
    fn toggles_detail_only_for_semantic_line() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.apply_update(ViewUpdate::Semantic {
            text: "Waiting for approval".to_string(),
            details: vec!["run /permissions shell once".to_string()],
        });
        vm.focus = UiFocus::Timeline;
        vm.selected_line = 0;
        vm.toggle_detail();
        assert!(vm.lines[0].expanded);
    }

    #[test]
    fn keeps_session_id_and_supports_system_line_kind() {
        let mut vm = TuiViewModel::new("session-42".to_string());
        assert_eq!(vm.session_id, "session-42");
        vm.lines.push(UiLine {
            kind: UiLineKind::System,
            prefix: "i".to_string(),
            text: "system message".to_string(),
            details: Vec::new(),
            expanded: false,
        });
        assert_eq!(vm.lines[0].kind, UiLineKind::System);
    }
}
