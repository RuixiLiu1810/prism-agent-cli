use crate::commands::{parse_ui_mode, Args, UiMode};

pub fn resolve_ui_mode(args: &Args) -> UiMode {
    args.ui_mode
        .as_deref()
        .and_then(|raw| parse_ui_mode(raw).ok())
        .unwrap_or(UiMode::Tui)
}
