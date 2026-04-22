use std::io::Write;

use crate::status_snapshot::CliStatusSnapshot;

pub fn render_header(snapshot: &CliStatusSnapshot) -> String {
    let dirty_suffix = if snapshot.git_dirty { "*" } else { "" };
    let line_one = format!(
        "{} / {} | {} | {}",
        snapshot.provider, snapshot.model, snapshot.output_mode, snapshot.session_id
    );
    let line_two = format!(
        "{} | {}{}",
        snapshot.project_path, snapshot.git_branch, dirty_suffix
    );

    format!(
        "\n=== agent-runtime ==============================================\n{}\n{}\n===============================================================\n",
        line_one, line_two
    )
}

pub fn print_header<W: Write>(writer: &mut W, snapshot: &CliStatusSnapshot) -> Result<(), String> {
    writer
        .write_all(render_header(snapshot).as_bytes())
        .map_err(|e| format!("failed to write header: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("failed to flush header: {e}"))
}

pub fn clear_screen<W: Write>(writer: &mut W) -> Result<(), String> {
    writer
        .write_all(b"\x1b[2J\x1b[H")
        .map_err(|e| format!("failed to clear screen: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("failed to flush clear screen: {e}"))
}

#[cfg(test)]
mod tests {
    use super::render_header;
    use crate::status_snapshot::CliStatusSnapshot;

    #[test]
    fn renders_header_with_expected_fields() {
        let snapshot = CliStatusSnapshot {
            provider: "minimax".to_string(),
            model: "MiniMax-M1".to_string(),
            project_path: "/tmp/project".to_string(),
            git_branch: "main".to_string(),
            git_dirty: true,
            session_id: "session-1".to_string(),
            output_mode: "human".to_string(),
        };

        let header = render_header(&snapshot);
        assert!(header.contains("minimax / MiniMax-M1 | human | session-1"));
        assert!(header.contains("/tmp/project | main*"));
    }
}
