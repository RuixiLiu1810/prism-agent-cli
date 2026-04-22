use crate::config_model::{default_base_url, ResolvedConfig, StoredConfig};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingField {
    Provider,
    Model,
    ApiKey,
}

pub fn merge_sources(cli: &RawConfig, env: &RawConfig, file: &RawConfig) -> RawConfig {
    RawConfig {
        provider: cli
            .provider
            .clone()
            .or_else(|| env.provider.clone())
            .or_else(|| file.provider.clone()),
        model: cli
            .model
            .clone()
            .or_else(|| env.model.clone())
            .or_else(|| file.model.clone()),
        api_key: cli
            .api_key
            .clone()
            .or_else(|| env.api_key.clone())
            .or_else(|| file.api_key.clone()),
        base_url: cli
            .base_url
            .clone()
            .or_else(|| env.base_url.clone())
            .or_else(|| file.base_url.clone()),
        output: cli
            .output
            .clone()
            .or_else(|| env.output.clone())
            .or_else(|| file.output.clone()),
    }
}

pub fn detect_missing(raw: &RawConfig) -> Vec<MissingField> {
    let mut missing = Vec::new();

    if raw
        .provider
        .as_deref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        missing.push(MissingField::Provider);
    }
    if raw
        .model
        .as_deref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        missing.push(MissingField::Model);
    }
    if raw
        .api_key
        .as_deref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        missing.push(MissingField::ApiKey);
    }

    missing
}

pub fn finalize(raw: RawConfig) -> Result<ResolvedConfig, String> {
    let provider = raw
        .provider
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "provider is required".to_string())?;

    let model = raw
        .model
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "model is required".to_string())?;

    let api_key = raw
        .api_key
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "api_key is required".to_string())?;

    let base_url = raw
        .base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| default_base_url(&provider).map(|v| v.to_string()))
        .ok_or_else(|| format!("unsupported provider '{provider}'"))?;

    let output = raw
        .output
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "human".to_string());

    Ok(ResolvedConfig {
        provider,
        model,
        api_key,
        base_url,
        output,
    })
}

pub fn file_to_raw(file: Option<StoredConfig>) -> RawConfig {
    if let Some(cfg) = file {
        RawConfig {
            provider: cfg.provider,
            model: cfg.model,
            api_key: cfg.api_key,
            base_url: cfg.base_url,
            output: cfg.output,
        }
    } else {
        RawConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_missing, finalize, merge_sources, MissingField, RawConfig};

    #[test]
    fn cli_overrides_env_and_file() {
        let cli = RawConfig {
            provider: Some("deepseek".to_string()),
            model: None,
            api_key: None,
            base_url: None,
            output: None,
        };
        let env = RawConfig {
            provider: Some("minimax".to_string()),
            model: Some("MiniMax-M1".to_string()),
            api_key: Some("env-key".to_string()),
            base_url: None,
            output: None,
        };
        let file = RawConfig {
            provider: Some("minimax".to_string()),
            model: Some("file-model".to_string()),
            api_key: Some("file-key".to_string()),
            base_url: None,
            output: Some("jsonl".to_string()),
        };

        let merged = merge_sources(&cli, &env, &file);
        assert_eq!(merged.provider.as_deref(), Some("deepseek"));
        assert_eq!(merged.model.as_deref(), Some("MiniMax-M1"));
        assert_eq!(merged.api_key.as_deref(), Some("env-key"));
        assert_eq!(merged.output.as_deref(), Some("jsonl"));
    }

    #[test]
    fn detect_missing_fields() {
        let missing = detect_missing(&RawConfig::default());
        assert_eq!(missing, vec![MissingField::Provider, MissingField::Model, MissingField::ApiKey]);
    }

    #[test]
    fn finalize_builds_defaults() {
        let resolved = finalize(RawConfig {
            provider: Some("minimax".to_string()),
            model: Some("MiniMax-M1".to_string()),
            api_key: Some("k".to_string()),
            base_url: None,
            output: None,
        })
        .unwrap_or_else(|e| panic!("finalize: {e}"));

        assert_eq!(resolved.base_url, "https://api.minimax.chat/v1");
        assert_eq!(resolved.output, "human");
    }
}
