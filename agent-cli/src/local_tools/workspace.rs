use agent_core::{is_document_resource_path, tools::error_result, AgentToolResult};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::watch;

use super::common::{
    cancelled_result, command_output_with_cancel, files_preview, is_cancelled, ok_result,
    resolve_project_path, tool_arg_optional_string, tool_arg_string, truncate_file_bytes,
    MAX_LISTED_FILES, MAX_SEARCH_LINES,
};

pub(crate) async fn execute_read_file(
    project_root: &str,
    call_id: &str,
    args: Value,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    if is_cancelled(cancel_rx.as_ref()) {
        return cancelled_result("read_file", call_id);
    }

    let raw_path = match tool_arg_string(&args, "path") {
        Ok(value) => value,
        Err(message) => return error_result("read_file", call_id, message),
    };

    if is_document_resource_path(&raw_path) {
        return error_result(
            "read_file",
            call_id,
            format!(
                "{} is a document resource, not a plain text file. Use read_document instead.",
                raw_path
            ),
        );
    }

    let full_path = match resolve_project_path(project_root, &raw_path) {
        Ok(path) => path,
        Err(message) => return error_result("read_file", call_id, message),
    };

    let bytes = match tokio::fs::read(&full_path).await {
        Ok(bytes) => bytes,
        Err(err) => {
            return error_result(
                "read_file",
                call_id,
                format!("Failed to read {}: {}", raw_path, err),
            );
        }
    };

    let (slice, truncated) = truncate_file_bytes(&bytes);
    let content = String::from_utf8_lossy(slice).to_string();
    let preview = format!(
        "{}{}",
        content,
        if truncated { "\n...[truncated]" } else { "" }
    );

    ok_result(
        "read_file",
        call_id,
        json!({
            "path": raw_path,
            "content": content,
            "truncated": truncated,
            "byteCount": bytes.len(),
        }),
        preview,
    )
}

pub(crate) async fn execute_list_files(
    project_root: &str,
    call_id: &str,
    args: Value,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    let path = tool_arg_optional_string(&args, "path").unwrap_or_else(|| ".".to_string());
    let search_root = match resolve_project_path(project_root, &path) {
        Ok(path) => path,
        Err(message) => return error_result("list_files", call_id, message),
    };

    let mut command = Command::new("rg");
    command
        .arg("--files")
        .arg(&search_root)
        .current_dir(project_root);

    let output = match command_output_with_cancel(
        command,
        cancel_rx,
        "list_files",
        call_id,
        "Failed to run rg --files",
    )
    .await
    {
        Ok(output) => output,
        Err(result) => return result,
    };

    if !output.status.success() {
        return error_result(
            "list_files",
            call_id,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        );
    }

    let root = std::path::Path::new(project_root)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(project_root));

    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let line_path = std::path::Path::new(line);
            line_path
                .strip_prefix(&root)
                .ok()
                .map(|relative| relative.to_string_lossy().to_string())
                .or_else(|| {
                    line_path
                        .strip_prefix(project_root)
                        .ok()
                        .map(|relative| relative.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| line.to_string())
                .trim_start_matches('/')
                .to_string()
        })
        .collect::<Vec<_>>();
    files.sort();

    let truncated = files.len() > MAX_LISTED_FILES;
    if truncated {
        files.truncate(MAX_LISTED_FILES);
    }

    ok_result(
        "list_files",
        call_id,
        json!({
            "path": path,
            "files": files,
            "truncated": truncated,
        }),
        files_preview("Files", &files, truncated),
    )
}

pub(crate) async fn execute_search_project(
    project_root: &str,
    call_id: &str,
    args: Value,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> AgentToolResult {
    let query = match tool_arg_string(&args, "query") {
        Ok(value) => value,
        Err(message) => return error_result("search_project", call_id, message),
    };
    let path = tool_arg_optional_string(&args, "path").unwrap_or_else(|| ".".to_string());
    let search_root = match resolve_project_path(project_root, &path) {
        Ok(path) => path,
        Err(message) => return error_result("search_project", call_id, message),
    };

    let mut command = Command::new("rg");
    command
        .arg("-n")
        .arg("--no-heading")
        .arg("--color")
        .arg("never")
        .arg(&query)
        .arg(&search_root)
        .current_dir(project_root);

    let output = match command_output_with_cancel(
        command,
        cancel_rx,
        "search_project",
        call_id,
        "Failed to run ripgrep",
    )
    .await
    {
        Ok(output) => output,
        Err(result) => return result,
    };

    if output.status.code().unwrap_or(-1) > 1 {
        return error_result(
            "search_project",
            call_id,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        );
    }

    let mut matches = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let truncated = matches.len() > MAX_SEARCH_LINES;
    if truncated {
        matches.truncate(MAX_SEARCH_LINES);
    }

    ok_result(
        "search_project",
        call_id,
        json!({
            "query": query,
            "path": path,
            "matches": matches,
            "truncated": truncated,
        }),
        files_preview("Matches", &matches, truncated),
    )
}

#[cfg(test)]
mod tests {
    use super::{execute_read_file, execute_search_project};

    #[tokio::test]
    async fn read_file_returns_content_for_text_file() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let file = dir.path().join("src.txt");
        tokio::fs::write(&file, "hello")
            .await
            .unwrap_or_else(|e| panic!("write: {e}"));

        let result = execute_read_file(
            dir.path().to_str().unwrap_or("."),
            "call-1",
            serde_json::json!({"path":"src.txt"}),
            None,
        )
        .await;

        assert!(!result.is_error);
        assert_eq!(result.content["content"], "hello");
    }

    #[tokio::test]
    async fn read_file_blocks_path_traversal() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let result = execute_read_file(
            dir.path().to_str().unwrap_or("."),
            "call-1",
            serde_json::json!({"path":"../secret.txt"}),
            None,
        )
        .await;

        assert!(result.is_error);
    }

    #[tokio::test]
    async fn search_project_returns_matches() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let file = dir.path().join("notes.txt");
        tokio::fs::write(&file, "alpha\nbeta\nalpha")
            .await
            .unwrap_or_else(|e| panic!("write: {e}"));

        let result = execute_search_project(
            dir.path().to_str().unwrap_or("."),
            "call-1",
            serde_json::json!({"query":"alpha"}),
            None,
        )
        .await;

        if result.is_error {
            // Environment may lack rg; make failure explicit instead of flaky panic.
            let message = result
                .content
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            assert!(
                message.contains("Failed to run ripgrep")
                    || message.contains("No such file or directory")
                    || message.contains("not found"),
                "unexpected error: {}",
                message
            );
            return;
        }

        let matches = result
            .content
            .get("matches")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(!matches.is_empty());
    }
}
