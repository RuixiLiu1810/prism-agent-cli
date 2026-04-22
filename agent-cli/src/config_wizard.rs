use std::io::{self, Write};

use crate::config_model::{default_base_url, StoredConfig};

pub trait WizardIo {
    fn print_line(&mut self, line: &str) -> Result<(), String>;
    fn read_line(&mut self, prompt: &str) -> Result<String, String>;
}

#[derive(Default)]
pub struct StdioWizardIo;

impl WizardIo for StdioWizardIo {
    fn print_line(&mut self, line: &str) -> Result<(), String> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        writeln!(handle, "{}", line).map_err(|e| format!("write wizard output failed: {e}"))
    }

    fn read_line(&mut self, prompt: &str) -> Result<String, String> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write!(handle, "{}: ", prompt).map_err(|e| format!("write prompt failed: {e}"))?;
        handle
            .flush()
            .map_err(|e| format!("flush prompt failed: {e}"))?;

        let stdin = io::stdin();
        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .map_err(|e| format!("read wizard input failed: {e}"))?;
        Ok(input.trim().to_string())
    }
}

pub fn run_wizard(
    io: &mut dyn WizardIo,
    existing: Option<&StoredConfig>,
) -> Result<StoredConfig, String> {
    io.print_line("")?;
    io.print_line("========================================")?;
    io.print_line(" Agent Runtime Setup Wizard")?;
    io.print_line("========================================")?;

    let provider = ask_provider(io, existing.and_then(|c| c.provider.as_deref()))?;
    let default_model = if provider == "minimax" {
        "MiniMax-M1"
    } else {
        "deepseek-chat"
    };
    let model = ask_text(
        io,
        "Model",
        existing
            .and_then(|c| c.model.as_deref())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(default_model),
        false,
    )?;

    let api_key = ask_text(
        io,
        "API Key",
        existing
            .and_then(|c| c.api_key.as_deref())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(""),
        false,
    )?;

    let default_url = default_base_url(&provider).unwrap_or("");
    let base_url = ask_text(
        io,
        "Base URL",
        existing
            .and_then(|c| c.base_url.as_deref())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(default_url),
        false,
    )?;

    let output = ask_text(
        io,
        "Output (human/jsonl)",
        existing
            .and_then(|c| c.output.as_deref())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("human"),
        false,
    )?;

    io.print_line("")?;
    io.print_line("Review:")?;
    io.print_line(&format!("- provider: {}", provider))?;
    io.print_line(&format!("- model: {}", model))?;
    io.print_line("- api_key: ***")?;
    io.print_line(&format!("- base_url: {}", base_url))?;
    io.print_line(&format!("- output: {}", output))?;
    io.print_line("Type 'save' to persist, or 'cancel' to abort")?;

    let action = io.read_line("Action")?;
    if action.trim().eq_ignore_ascii_case("save") {
        Ok(StoredConfig {
            provider: Some(provider),
            model: Some(model),
            api_key: Some(api_key),
            base_url: Some(base_url),
            output: Some(output),
        })
    } else {
        Err("wizard cancelled".to_string())
    }
}

fn ask_provider(io: &mut dyn WizardIo, existing: Option<&str>) -> Result<String, String> {
    io.print_line("")?;
    io.print_line("Choose provider:")?;
    io.print_line("1) minimax")?;
    io.print_line("2) deepseek")?;

    let default = existing
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "minimax" || v == "deepseek")
        .unwrap_or_else(|| "minimax".to_string());

    loop {
        let answer = io.read_line(&format!("Provider [default: {}]", default))?;
        let normalized = if answer.trim().is_empty() {
            default.clone()
        } else {
            match answer.trim() {
                "1" => "minimax".to_string(),
                "2" => "deepseek".to_string(),
                other => other.to_ascii_lowercase(),
            }
        };

        if normalized == "minimax" || normalized == "deepseek" {
            return Ok(normalized);
        }

        io.print_line("Invalid provider. Use minimax/deepseek or 1/2.")?;
    }
}

fn ask_text(
    io: &mut dyn WizardIo,
    label: &str,
    default: &str,
    allow_empty: bool,
) -> Result<String, String> {
    loop {
        let prompt = if default.is_empty() {
            label.to_string()
        } else {
            format!("{} [default: {}]", label, default)
        };

        let input = io.read_line(&prompt)?;
        let value = if input.trim().is_empty() {
            default.to_string()
        } else {
            input.trim().to_string()
        };

        if allow_empty || !value.trim().is_empty() {
            return Ok(value);
        }

        io.print_line("Value cannot be empty.")?;
    }
}

#[cfg(test)]
mod tests {
    use super::{run_wizard, WizardIo};
    use crate::config_model::StoredConfig;

    struct FakeWizardIo {
        inputs: Vec<String>,
        output: Vec<String>,
    }

    impl FakeWizardIo {
        fn new(inputs: Vec<String>) -> Self {
            Self {
                inputs: inputs.into_iter().rev().collect(),
                output: Vec::new(),
            }
        }
    }

    impl WizardIo for FakeWizardIo {
        fn print_line(&mut self, line: &str) -> Result<(), String> {
            self.output.push(line.to_string());
            Ok(())
        }

        fn read_line(&mut self, _prompt: &str) -> Result<String, String> {
            self.inputs
                .pop()
                .ok_or_else(|| "missing fake input".to_string())
        }
    }

    #[test]
    fn wizard_collects_required_fields_in_order() {
        let mut io = FakeWizardIo::new(vec![
            "1".to_string(),
            "MiniMax-M1".to_string(),
            "sk-test".to_string(),
            "".to_string(),
            "human".to_string(),
            "save".to_string(),
        ]);

        let cfg = run_wizard(&mut io, None).unwrap_or_else(|e| panic!("wizard failed: {e}"));
        assert_eq!(cfg.provider.as_deref(), Some("minimax"));
        assert_eq!(cfg.model.as_deref(), Some("MiniMax-M1"));
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
        assert_eq!(cfg.base_url.as_deref(), Some("https://api.minimax.chat/v1"));
    }

    #[test]
    fn wizard_uses_existing_defaults() {
        let existing = StoredConfig {
            provider: Some("deepseek".to_string()),
            model: Some("deepseek-chat".to_string()),
            api_key: Some("old-key".to_string()),
            base_url: Some("https://api.deepseek.com/v1".to_string()),
            output: Some("jsonl".to_string()),
        };

        let mut io = FakeWizardIo::new(vec![
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "save".to_string(),
        ]);

        let cfg =
            run_wizard(&mut io, Some(&existing)).unwrap_or_else(|e| panic!("wizard failed: {e}"));
        assert_eq!(cfg, existing);
    }
}
