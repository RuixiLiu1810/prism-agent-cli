use std::path::{Component, Path, PathBuf};
use std::process::Output;

use agent_core::{tools::error_result, AgentToolResult};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::watch;

pub(crate) const AGENT_CANCELLED_MESSAGE: &str = "Agent run cancelled by user.";
pub(crate) const MAX_FILE_BYTES: usize = 200_000;
pub(crate) const MAX_LISTED_FILES: usize = 500;
pub(crate) const MAX_SEARCH_LINES: usize = 200;

pub(crate) fn tool_arg_string(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Missing required tool argument '{}'.", key))
}

pub(crate) fn tool_arg_optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn resolve_project_path(project_root: &str, raw_path: &str) -> Result<PathBuf, String> {
    let root = Path::new(project_root)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root '{}': {}", project_root, e))?;

    let mut relative = PathBuf::new();
    for component in Path::new(raw_path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !relative.pop() {
                    return Err(format!("Path escapes project root: {}", raw_path));
                }
            }
            Component::Normal(value) => relative.push(value),
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!("Absolute paths are not allowed: {}", raw_path));
            }
        }
    }

    let resolved = root.join(relative);
    if !resolved.starts_with(&root) {
        return Err(format!("Path escapes project root: {}", raw_path));
    }

    Ok(resolved)
}

pub(crate) fn ok_result(
    tool_name: &str,
    call_id: &str,
    content: Value,
    preview: String,
) -> AgentToolResult {
    AgentToolResult {
        tool_name: tool_name.to_string(),
        call_id: call_id.to_string(),
        is_error: false,
        content,
        preview: truncate_preview(&preview),
    }
}

pub(crate) fn cancelled_result(tool_name: &str, call_id: &str) -> AgentToolResult {
    error_result(tool_name, call_id, AGENT_CANCELLED_MESSAGE.to_string())
}

pub(crate) fn is_cancelled(cancel_rx: Option<&watch::Receiver<bool>>) -> bool {
    cancel_rx.map(|rx| *rx.borrow()).unwrap_or(false)
}

pub(crate) fn truncate_preview(raw: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 240;
    if raw.chars().count() <= MAX_PREVIEW_CHARS {
        raw.to_string()
    } else {
        let mut clipped = raw.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        clipped.push_str("...");
        clipped
    }
}

pub(crate) fn truncate_file_bytes(bytes: &[u8]) -> (&[u8], bool) {
    if bytes.len() <= MAX_FILE_BYTES {
        (bytes, false)
    } else {
        (&bytes[..MAX_FILE_BYTES], true)
    }
}

pub(crate) async fn command_output_with_cancel(
    mut command: Command,
    mut cancel_rx: Option<watch::Receiver<bool>>,
    tool_name: &str,
    call_id: &str,
    spawn_error_prefix: &str,
) -> Result<Output, AgentToolResult> {
    if is_cancelled(cancel_rx.as_ref()) {
        return Err(cancelled_result(tool_name, call_id));
    }

    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command.kill_on_drop(true);
    let child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return Err(error_result(
                tool_name,
                call_id,
                format!("{}: {}", spawn_error_prefix, err),
            ));
        }
    };

    let output_fut = child.wait_with_output();
    tokio::pin!(output_fut);

    if let Some(cancel_rx) = cancel_rx.as_mut() {
        loop {
            tokio::select! {
                changed = cancel_rx.changed() => {
                    match changed {
                        Ok(_) if *cancel_rx.borrow() => return Err(cancelled_result(tool_name, call_id)),
                        Ok(_) => continue,
                        Err(_) => return Err(cancelled_result(tool_name, call_id)),
                    }
                }
                output = &mut output_fut => {
                    return output.map_err(|e| {
                        error_result(tool_name, call_id, format!("Failed to wait for command: {}", e))
                    });
                }
            }
        }
    }

    output_fut.await.map_err(|e| {
        error_result(
            tool_name,
            call_id,
            format!("Failed to wait for command: {}", e),
        )
    })
}

pub(crate) fn files_preview(title: &str, lines: &[String], truncated: bool) -> String {
    let mut preview = format!("{} ({}):", title, lines.len());
    for line in lines.iter().take(10) {
        preview.push('\n');
        preview.push_str(line);
    }
    if truncated {
        preview.push_str("\n...[truncated]");
    }
    preview
}
