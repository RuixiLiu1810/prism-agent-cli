use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StoredConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub output: String,
}

pub fn default_base_url(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "minimax" => Some("https://api.minimax.chat/v1"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        _ => None,
    }
}

pub fn mask_secret(raw: &str) -> String {
    if raw.len() <= 5 {
        "*****".to_string()
    } else {
        format!("{}***{}", &raw[..3], &raw[raw.len() - 2..])
    }
}

#[cfg(test)]
mod tests {
    use super::{default_base_url, mask_secret};

    #[test]
    fn maps_provider_to_default_base_url() {
        assert_eq!(
            default_base_url("minimax"),
            Some("https://api.minimax.chat/v1")
        );
        assert_eq!(
            default_base_url("deepseek"),
            Some("https://api.deepseek.com/v1")
        );
        assert_eq!(default_base_url("openai"), None);
    }

    #[test]
    fn masks_api_key_in_show_output() {
        let masked = mask_secret("sk-test-secret");
        assert!(masked.starts_with("sk-"));
        assert!(masked.ends_with("et"));
        assert!(!masked.contains("test-secret"));
    }
}
