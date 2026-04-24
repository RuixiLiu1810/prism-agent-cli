use crate::status_snapshot::CliStatusSnapshot;

use super::view_model::TuiViewModel;

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    text.chars().take(width).collect()
}

pub fn render_header(snapshot: &CliStatusSnapshot, width: usize) -> String {
    let dirty = if snapshot.git_dirty { "*" } else { "" };
    let text = format!(
        "{}/{} | {} | {}{} | {} | {}",
        snapshot.provider,
        snapshot.model,
        snapshot.project_path,
        snapshot.git_branch,
        dirty,
        snapshot.session_id,
        snapshot.output_mode
    );
    truncate_to_width(&text, width)
}

pub fn render_frame(
    snapshot: &CliStatusSnapshot,
    vm: &TuiViewModel,
    width: u16,
    height: u16,
) -> Vec<String> {
    let width_usize = width as usize;
    let mut out = Vec::new();
    out.push(render_header(snapshot, width_usize));
    out.push("─".repeat(width_usize));

    let body_height = height.saturating_sub(4) as usize;
    let mut body_lines = Vec::new();
    for line in &vm.lines {
        body_lines.push(truncate_to_width(
            &format!("{} {}", line.prefix, line.text),
            width_usize,
        ));
        if line.expanded {
            for detail in &line.details {
                body_lines.push(truncate_to_width(&format!("  {}", detail), width_usize));
            }
        }
    }
    if body_lines.len() > body_height {
        body_lines = body_lines[body_lines.len() - body_height..].to_vec();
    }
    out.extend(body_lines);
    while out.len() < (height as usize).saturating_sub(2) {
        out.push(String::new());
    }
    out.push("─".repeat(width_usize));
    out.push(truncate_to_width(
        &format!("> {}", vm.input_buffer()),
        width_usize,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::{UiFocus, ViewUpdate};
    use crate::tui::view_model::TuiViewModel;

    #[test]
    fn renders_l1_header_with_required_fields() {
        let snapshot = CliStatusSnapshot {
            provider: "minimax".to_string(),
            model: "MiniMax-M1".to_string(),
            project_path: "/tmp/p".to_string(),
            git_branch: "main".to_string(),
            git_dirty: true,
            session_id: "session-1".to_string(),
            output_mode: "human".to_string(),
        };
        let vm = TuiViewModel::new("session-1".to_string());
        let lines = render_frame(&snapshot, &vm, 100, 24);
        assert!(lines.iter().any(|l| l.contains("minimax/MiniMax-M1")));
        assert!(lines.iter().any(|l| l.contains("main*")));
    }

    #[test]
    fn renders_expanded_detail_under_semantic_line() {
        let mut vm = TuiViewModel::new("session-1".to_string());
        vm.apply_update(ViewUpdate::Semantic {
            text: "Read src/main.rs".to_string(),
            details: vec!["tool=read_file path=src/main.rs".to_string()],
        });
        vm.focus = UiFocus::Timeline;
        vm.selected_line = 0;
        vm.toggle_detail();
        let snapshot = CliStatusSnapshot::collect("minimax", "MiniMax-M1", ".", "session-1", "human");
        let lines = render_frame(&snapshot, &vm, 120, 30);
        assert!(lines.iter().any(|l| l.contains("tool=read_file")));
    }
}
