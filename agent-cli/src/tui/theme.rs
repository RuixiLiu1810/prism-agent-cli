#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Text,
    Subtle,
    Warning,
    Error,
    Accent,
    CommandRowBg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub enable_color: bool,
}

impl Theme {
    pub fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let dumb_term = std::env::var("TERM")
            .ok()
            .is_some_and(|term| term.eq_ignore_ascii_case("dumb"));
        Self {
            enable_color: !(no_color || dumb_term),
        }
    }

    pub fn paint(&self, role: Role, text: impl AsRef<str>) -> String {
        let text = text.as_ref();
        if !self.enable_color {
            return text.to_string();
        }
        let code = match role {
            Role::Text => "0",
            Role::Subtle => "90",
            Role::Warning => "33",
            Role::Error => "31",
            Role::Accent => "36",
            Role::CommandRowBg => "48;5;254",
        };
        format!("\x1b[{}m{}\x1b[0m", code, text)
    }
}

#[cfg(test)]
mod tests {
    use super::{Role, Theme};

    #[test]
    fn paint_no_color_returns_plain_text() {
        let theme = Theme {
            enable_color: false,
        };
        assert_eq!(theme.paint(Role::Warning, "hello"), "hello");
    }

    #[test]
    fn paint_with_color_wraps_ansi() {
        let theme = Theme { enable_color: true };
        let output = theme.paint(Role::Accent, "ok");
        assert!(output.contains("\x1b[36m"));
        assert!(output.ends_with("\x1b[0m"));
    }

    #[test]
    fn paint_command_row_bg_uses_background_ansi_code() {
        let theme = Theme { enable_color: true };
        let output = theme.paint(Role::CommandRowBg, "› who are you");
        assert!(output.contains("\x1b[48;"));
    }
}
