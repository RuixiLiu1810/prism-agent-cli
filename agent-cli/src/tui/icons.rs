#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    pub tool: &'static str,
    pub semantic: &'static str,
    pub detail: &'static str,
    pub error: &'static str,
    pub waiting: &'static str,
    pub complete: &'static str,
}

impl Icons {
    pub fn project_logo() -> &'static str {
        "▗▄▖▗▄▖\n▐▌▚▞▐▌\n▐▌▞▚▐▌\n▝▚▞▝▚▞"
    }

    pub fn detect() -> Self {
        if prefers_ascii() {
            Self {
                tool: "[tool]",
                semantic: "[semantic]",
                detail: "[detail]",
                error: "[error]",
                waiting: "[wait]",
                complete: "[done]",
            }
        } else {
            Self {
                tool: "⚙",
                semantic: "●",
                detail: "└",
                error: "✖",
                waiting: "⏸",
                complete: "✓",
            }
        }
    }

    pub fn spinner_frame(&self, tick: usize, reduced_motion: bool) -> &'static str {
        if reduced_motion {
            return if self.tool == "⚙" { "•" } else { "*" };
        }
        const UNICODE_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        const ASCII_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
        if self.tool == "⚙" {
            UNICODE_FRAMES[tick % UNICODE_FRAMES.len()]
        } else {
            ASCII_FRAMES[tick % ASCII_FRAMES.len()]
        }
    }
}

pub fn reduced_motion_enabled() -> bool {
    std::env::var("AGENT_REDUCED_MOTION")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}

fn prefers_ascii() -> bool {
    if std::env::var("AGENT_ASCII_ONLY")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
    {
        return true;
    }
    let lang = std::env::var("LANG").unwrap_or_default().to_ascii_uppercase();
    !lang.contains("UTF-8")
}

#[cfg(test)]
mod tests {
    use super::{Icons, reduced_motion_enabled};

    #[test]
    fn project_logo_is_unicode_pixel_block() {
        let logo = Icons::project_logo();
        assert_eq!(logo.lines().count(), 4);
        assert!(logo.contains("▗"));
    }

    #[test]
    fn spinner_uses_static_frame_when_reduced_motion() {
        let icons = Icons::detect();
        let frame = icons.spinner_frame(2, true);
        assert!(!frame.is_empty());
    }

    #[test]
    fn reduced_motion_defaults_to_false() {
        assert!(!reduced_motion_enabled());
    }
}
