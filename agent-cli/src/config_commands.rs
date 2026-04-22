use crate::args::ConfigSubcommand;
use crate::config_model::{mask_secret, StoredConfig};
use crate::config_store::{default_config_path, load_config, save_config_atomic};
use crate::config_wizard::{run_wizard, WizardIo};

pub fn execute_config_command(
    action: &ConfigSubcommand,
    io: &mut dyn WizardIo,
) -> Result<String, String> {
    let path = default_config_path()?;

    match action {
        ConfigSubcommand::Path => Ok(path.display().to_string()),
        ConfigSubcommand::Show => {
            let cfg = load_config(&path)?.unwrap_or_default();
            Ok(render_show(&cfg))
        }
        ConfigSubcommand::Init => {
            let cfg = run_wizard(io, None)?;
            save_config_atomic(&path, &cfg)?;
            Ok(format!("Config saved: {}", path.display()))
        }
        ConfigSubcommand::Edit => {
            let existing = load_config(&path)?;
            let cfg = run_wizard(io, existing.as_ref())?;
            save_config_atomic(&path, &cfg)?;
            Ok(format!("Config updated: {}", path.display()))
        }
    }
}

pub fn render_show(cfg: &StoredConfig) -> String {
    let api = cfg
        .api_key
        .as_deref()
        .map(mask_secret)
        .unwrap_or_else(|| "<unset>".to_string());

    format!(
        "provider: {}\nmodel: {}\napi_key: {}\nbase_url: {}\noutput: {}",
        cfg.provider.as_deref().unwrap_or("<unset>"),
        cfg.model.as_deref().unwrap_or("<unset>"),
        api,
        cfg.base_url.as_deref().unwrap_or("<unset>"),
        cfg.output.as_deref().unwrap_or("human")
    )
}

#[cfg(test)]
mod tests {
    use super::render_show;
    use crate::config_model::StoredConfig;

    #[test]
    fn show_masks_api_key() {
        let cfg = StoredConfig {
            provider: Some("minimax".to_string()),
            model: Some("MiniMax-M1".to_string()),
            api_key: Some("sk-very-secret".to_string()),
            base_url: Some("https://api.minimax.chat/v1".to_string()),
            output: Some("human".to_string()),
        };
        let shown = render_show(&cfg);
        assert!(shown.contains("sk-***et"));
        assert!(!shown.contains("very-secret"));
    }
}
