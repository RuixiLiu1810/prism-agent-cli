use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliStatusSnapshot {
    pub provider: String,
    pub model: String,
    pub project_path: String,
    pub git_branch: String,
    pub git_dirty: bool,
    pub session_id: String,
    pub output_mode: String,
}

impl CliStatusSnapshot {
    pub fn collect(
        provider: &str,
        model: &str,
        project_path: &str,
        session_id: &str,
        output_mode: &str,
    ) -> Self {
        let normalized_project_path = normalize_project_path(project_path);
        let (git_branch, git_dirty) = detect_git_status(&normalized_project_path);

        Self {
            provider: fallback(provider),
            model: fallback(model),
            project_path: normalized_project_path,
            git_branch,
            git_dirty,
            session_id: fallback(session_id),
            output_mode: fallback(output_mode),
        }
    }
}

fn normalize_project_path(raw: &str) -> String {
    let path = raw.trim();
    if path.is_empty() {
        return "<unset>".to_string();
    }

    Path::new(path)
        .canonicalize()
        .map(|v| v.display().to_string())
        .unwrap_or_else(|_| path.to_string())
}

fn detect_git_status(project_path: &str) -> (String, bool) {
    if project_path == "<unset>" {
        return ("<no-git>".to_string(), false);
    }

    let inside_repo = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();

    let Ok(inside_repo) = inside_repo else {
        return ("<no-git>".to_string(), false);
    };

    if !inside_repo.status.success() {
        return ("<no-git>".to_string(), false);
    }

    let branch = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .arg("branch")
        .arg("--show-current")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            } else {
                None
            }
        })
        .unwrap_or_else(|| "HEAD".to_string());

    let dirty = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .arg("status")
        .arg("--porcelain")
        .output()
        .ok()
        .map(|output| {
            if output.status.success() {
                !String::from_utf8_lossy(&output.stdout).trim().is_empty()
            } else {
                false
            }
        })
        .unwrap_or(false);

    (branch, dirty)
}

fn fallback(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        "<unset>".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::CliStatusSnapshot;
    use std::fs;

    #[test]
    fn falls_back_when_project_is_not_repo() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let snapshot = CliStatusSnapshot::collect(
            "minimax",
            "MiniMax-M1",
            dir.path().to_str().unwrap_or(""),
            "session-1",
            "human",
        );

        assert_eq!(snapshot.git_branch, "<no-git>");
        assert!(!snapshot.git_dirty);
    }

    #[test]
    fn detects_git_branch_and_dirty_state() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let status = std::process::Command::new("git")
            .arg("init")
            .arg(dir.path())
            .status()
            .unwrap_or_else(|e| panic!("git init: {e}"));
        assert!(status.success());

        let dirty_file = dir.path().join("dirty.txt");
        fs::write(&dirty_file, "changes").unwrap_or_else(|e| panic!("write dirty file: {e}"));

        let snapshot = CliStatusSnapshot::collect(
            "minimax",
            "MiniMax-M1",
            dir.path().to_str().unwrap_or(""),
            "session-1",
            "human",
        );

        assert_ne!(snapshot.git_branch, "<no-git>");
        assert!(snapshot.git_dirty);
    }
}
