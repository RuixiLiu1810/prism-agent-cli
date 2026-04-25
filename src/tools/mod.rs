use crate::commands::ToolMode;

pub fn mode_label(mode: ToolMode) -> &'static str {
    match mode {
        ToolMode::Off => "off",
        ToolMode::Safe => "safe",
    }
}
