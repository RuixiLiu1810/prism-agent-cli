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
        (KeyCode::Esc, _) => Some(UiAction::FocusInput),
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
            vm.input_buffer.push(ch);
            None
        }
        UiAction::Backspace => {
            vm.input_buffer.pop();
            None
        }
        UiAction::Enter => {
            if vm.focus == UiFocus::Timeline {
                vm.toggle_detail();
                return None;
            }
            let prompt = vm.input_buffer.trim().to_string();
            vm.input_buffer.clear();
            if prompt.is_empty() { None } else { Some(prompt) }
        }
        UiAction::HistoryUp => {
            if vm.input_history.is_empty() {
                return None;
            }
            let next = match vm.history_cursor {
                Some(cursor) if cursor > 0 => cursor - 1,
                Some(cursor) => cursor,
                None => vm.input_history.len().saturating_sub(1),
            };
            vm.history_cursor = Some(next);
            if let Some(value) = vm.input_history.get(next) {
                vm.input_buffer = value.clone();
            }
            None
        }
        UiAction::HistoryDown => {
            if vm.input_history.is_empty() {
                return None;
            }
            if let Some(cursor) = vm.history_cursor {
                let next = cursor.saturating_add(1);
                if next >= vm.input_history.len() {
                    vm.history_cursor = None;
                    vm.input_buffer.clear();
                } else {
                    vm.history_cursor = Some(next);
                    if let Some(value) = vm.input_history.get(next) {
                        vm.input_buffer = value.clone();
                    }
                }
            }
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
}
