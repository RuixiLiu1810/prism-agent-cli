#[path = "../../agent-cli/src/args.rs"]
pub mod args;

use std::collections::HashMap;

use crate::services::turn_service::AppContext;

pub use args::{
    parse_output_mode, parse_tool_mode, parse_ui_mode, Args, Command, ConfigSubcommand, OutputMode,
    RunMode, ToolMode, UiMode,
};

pub type CommandHandler = fn(&mut AppContext, &[&str]) -> Result<(), String>;

fn handle_help(_ctx: &mut AppContext, _args: &[&str]) -> Result<(), String> {
    Ok(())
}

fn handle_status(_ctx: &mut AppContext, _args: &[&str]) -> Result<(), String> {
    Ok(())
}

pub fn registry() -> HashMap<&'static str, CommandHandler> {
    let mut map = HashMap::new();
    map.insert("/help", handle_help as CommandHandler);
    map.insert("/status", handle_status as CommandHandler);
    map
}
