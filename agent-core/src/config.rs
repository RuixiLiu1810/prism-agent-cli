use std::path::PathBuf;

// ── Config data types (mirror of settings/types.rs, now public) ─────

#[derive(Debug, Clone)]
pub struct AgentSamplingConfig {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct AgentSamplingProfilesConfig {
    pub edit_stable: AgentSamplingConfig,
    pub analysis_balanced: AgentSamplingConfig,
    pub analysis_deep: AgentSamplingConfig,
    pub chat_flexible: AgentSamplingConfig,
}

#[derive(Debug, Clone)]
pub struct AgentDomainConfig {
    pub domain: String,
    pub custom_instructions: Option<String>,
    pub terminology_strictness: String,
}

#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    pub runtime: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub domain_config: AgentDomainConfig,
    pub sampling_profiles: AgentSamplingProfilesConfig,
}

// ── ConfigProvider trait ─────────────────────────────────────────────

/// Abstraction over configuration loading — replaces `settings::load_agent_runtime(app, ...)`.
///
/// Tauri adapter reads from the layered settings system (global / project / secret).
/// CLI adapter reads from env vars + a config file.
/// Tests use `StaticConfigProvider`.
pub trait ConfigProvider: Send + Sync {
    /// Load the agent runtime configuration.
    /// `project_root`: optional project directory for project-scoped overrides.
    fn load_agent_runtime(
        &self,
        project_root: Option<&str>,
    ) -> Result<AgentRuntimeConfig, String>;

    /// Resolve the application config directory (for memory, logs, etc.).
    fn app_config_dir(&self) -> Result<PathBuf, String>;

    /// Resolve a storage directory for a given project.
    /// Returns the project-local `.chat-prism/` (or equivalent) path.
    fn project_storage_dir(&self, project_root: &str) -> Result<PathBuf, String>;
}

// ── Static implementation (for tests / CLI with pre-loaded config) ──

/// A config provider that returns a fixed `AgentRuntimeConfig`.
pub struct StaticConfigProvider {
    pub config: AgentRuntimeConfig,
    pub config_dir: PathBuf,
}

impl ConfigProvider for StaticConfigProvider {
    fn load_agent_runtime(
        &self,
        _project_root: Option<&str>,
    ) -> Result<AgentRuntimeConfig, String> {
        Ok(self.config.clone())
    }

    fn app_config_dir(&self) -> Result<PathBuf, String> {
        Ok(self.config_dir.clone())
    }

    fn project_storage_dir(&self, project_root: &str) -> Result<PathBuf, String> {
        Ok(PathBuf::from(project_root).join(".chat-prism"))
    }
}

// ── Default config (sensible defaults for quick start) ──────────────

impl AgentRuntimeConfig {
    pub fn default_local_agent() -> Self {
        Self {
            runtime: "local_agent".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            domain_config: AgentDomainConfig {
                domain: "general".to_string(),
                custom_instructions: None,
                terminology_strictness: "moderate".to_string(),
            },
            sampling_profiles: AgentSamplingProfilesConfig {
                edit_stable: AgentSamplingConfig {
                    temperature: 0.2,
                    top_p: 0.9,
                    max_tokens: 8192,
                },
                analysis_balanced: AgentSamplingConfig {
                    temperature: 0.4,
                    top_p: 0.9,
                    max_tokens: 6144,
                },
                analysis_deep: AgentSamplingConfig {
                    temperature: 0.3,
                    top_p: 0.92,
                    max_tokens: 12288,
                },
                chat_flexible: AgentSamplingConfig {
                    temperature: 0.7,
                    top_p: 0.95,
                    max_tokens: 4096,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn static_provider_returns_config() {
        let provider = StaticConfigProvider {
            config: AgentRuntimeConfig::default_local_agent(),
            config_dir: PathBuf::from("/tmp/test-config"),
        };
        let cfg = provider.load_agent_runtime(None).unwrap();
        assert_eq!(cfg.runtime, "local_agent");
        assert_eq!(cfg.model, "gpt-5.4");
    }

    #[test]
    fn static_provider_project_storage_dir() {
        let provider = StaticConfigProvider {
            config: AgentRuntimeConfig::default_local_agent(),
            config_dir: PathBuf::from("/tmp"),
        };
        let dir = provider.project_storage_dir("/home/user/project").unwrap();
        assert_eq!(dir, Path::new("/home/user/project/.chat-prism"));
    }

    #[test]
    fn default_config_has_sensible_values() {
        let cfg = AgentRuntimeConfig::default_local_agent();
        assert!(cfg.sampling_profiles.edit_stable.temperature < 0.5);
        assert!(cfg.sampling_profiles.chat_flexible.temperature > 0.5);
        assert!(cfg.api_key.is_none());
    }
}
