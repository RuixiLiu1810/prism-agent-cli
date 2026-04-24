use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::types::UiFocus;
use super::view_model::TuiViewModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    Enter,
    Backspace,
    InsertChar(char),
    HistoryUp,
    HistoryDown,
    StartHistorySearch,
    Undo,
    FocusNextLine,
    FocusPrevLine,
    ToggleDetail,
    FocusInput,
    ClearScreen,
    Exit,
    Noop,
}

pub fn to_action(key: KeyEvent) -> Option<UiAction> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(UiAction::Enter),
        (KeyCode::Backspace, _) => Some(UiAction::Backspace),
        (KeyCode::Up, _) => Some(UiAction::HistoryUp),
        (KeyCode::Down, _) => Some(UiAction::HistoryDown),
        (KeyCode::Char('r'), KeyModifiers::CONTROL) => Some(UiAction::StartHistorySearch),
        (KeyCode::Char('z'), KeyModifiers::CONTROL) => Some(UiAction::Undo),
        (KeyCode::Esc, _) => Some(UiAction::FocusInput),
        (KeyCode::Tab, _) => Some(UiAction::ToggleDetail),
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => Some(UiAction::ClearScreen),
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Some(UiAction::FocusNextLine),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(UiAction::FocusPrevLine),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(UiAction::Exit),
        (KeyCode::Char(ch), KeyModifiers::NONE) => Some(UiAction::InsertChar(ch)),
        _ => Some(UiAction::Noop),
    }
}

pub fn apply_input_action(vm: &mut TuiViewModel, action: UiAction) -> Option<String> {
    match action {
        UiAction::InsertChar(ch) => {
            vm.input.insert_char(ch);
            None
        }
        UiAction::Backspace => {
            vm.input.backspace();
            None
        }
        UiAction::Enter => {
            if vm.focus == UiFocus::Timeline {
                vm.toggle_detail();
                return None;
            }
            vm.input.submit_trimmed()
        }
        UiAction::HistoryUp => {
            let current = vm.input.current().to_string();
            if let Some(value) = vm.history.up(&current) {
                vm.input.replace(value);
            }
            None
        }
        UiAction::HistoryDown => {
            if let Some(value) = vm.history.down() {
                vm.input.replace(value);
            }
            None
        }
        UiAction::StartHistorySearch => {
            let current = vm.input.current().to_string();
            if let Some(value) = vm.history.start_reverse_search(current) {
                vm.input.replace(value);
            }
            None
        }
        UiAction::Undo => {
            vm.input.undo();
            None
        }
        UiAction::FocusNextLine => {
            vm.focus = UiFocus::Timeline;
            vm.selected_line = (vm.selected_line + 1).min(vm.lines.len().saturating_sub(1));
            None
        }
        UiAction::FocusPrevLine => {
            vm.focus = UiFocus::Timeline;
            vm.selected_line = vm.selected_line.saturating_sub(1);
            None
        }
        UiAction::ToggleDetail => {
            vm.toggle_detail();
            None
        }
        UiAction::FocusInput => {
            vm.focus = UiFocus::Input;
            None
        }
        UiAction::ClearScreen | UiAction::Exit | UiAction::Noop => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::view_model::TuiViewModel;

    #[test]
    fn maps_ctrl_j_and_ctrl_k_to_timeline_navigation() {
        let down = to_action(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        let up = to_action(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        assert_eq!(down, Some(UiAction::FocusNextLine));
        assert_eq!(up, Some(UiAction::FocusPrevLine));
    }

    #[test]
    fn maps_enter_to_submit_when_input_focused() {
        let action = to_action(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Some(UiAction::Enter));
    }

    #[test]
    fn history_up_down_restores_draft_input() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.push_user_prompt("first".to_string());
        vm.push_user_prompt("second".to_string());
        vm.input.replace("draft");

        apply_input_action(&mut vm, UiAction::HistoryUp);
        assert_eq!(vm.input.current(), "second");
        apply_input_action(&mut vm, UiAction::HistoryDown);
        assert_eq!(vm.input.current(), "draft");
    }

    #[test]
    fn ctrl_z_undo_reverts_last_input_edit() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        apply_input_action(&mut vm, UiAction::InsertChar('a'));
        apply_input_action(&mut vm, UiAction::InsertChar('b'));
        apply_input_action(&mut vm, UiAction::Undo);
        assert_eq!(vm.input.current(), "a");
    }
}
