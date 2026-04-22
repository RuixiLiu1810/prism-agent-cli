use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config_model::StoredConfig;

pub fn default_config_path() -> Result<PathBuf, String> {
    if let Some(explicit) = std::env::var_os("AGENT_CONFIG_PATH") {
        return Ok(PathBuf::from(explicit));
    }

    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(base).join("claude-prism/agent-cli/config.json"));
    }

    let home = std::env::var_os("HOME")
        .ok_or_else(|| "HOME is not set; cannot resolve config path".to_string())?;
    Ok(PathBuf::from(home).join(".config/claude-prism/agent-cli/config.json"))
}

pub fn load_config(path: &Path) -> Result<Option<StoredConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|e| format!("read config failed: {e}"))?;
    match serde_json::from_str::<StoredConfig>(&content) {
        Ok(cfg) => Ok(Some(cfg)),
        Err(err) => {
            backup_corrupt_file(path)?;
            Err(format!("config parse failed: {err}"))
        }
    }
}

pub fn save_config_atomic(path: &Path, cfg: &StoredConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir failed: {e}"))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(cfg).map_err(|e| format!("encode config failed: {e}"))?;
    fs::write(&tmp_path, bytes).map_err(|e| format!("write temp config failed: {e}"))?;
    fs::rename(&tmp_path, path).map_err(|e| format!("commit config failed: {e}"))
}

fn backup_corrupt_file(path: &Path) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock error: {e}"))?
        .as_secs();
    let backup = path.with_extension(format!("json.bak.{stamp}"));
    fs::copy(path, &backup).map_err(|e| format!("backup corrupt config failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_config, save_config_atomic};
    use crate::config_model::StoredConfig;

    #[test]
    fn store_roundtrip_load_save() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("config.json");
        let cfg = StoredConfig {
            provider: Some("minimax".to_string()),
            model: Some("MiniMax-M1".to_string()),
            api_key: Some("k".to_string()),
            base_url: Some("https://api.minimax.chat/v1".to_string()),
            output: Some("human".to_string()),
        };

        save_config_atomic(&path, &cfg).unwrap_or_else(|e| panic!("save: {e}"));
        let loaded = load_config(&path).unwrap_or_else(|e| panic!("load: {e}"));
        assert_eq!(loaded, Some(cfg));
    }

    #[test]
    fn parse_error_creates_backup() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{invalid-json")
            .unwrap_or_else(|e| panic!("write invalid config: {e}"));

        let err = load_config(&path).err();
        assert!(err.is_some());

        let mut backup_count = 0usize;
        for entry in std::fs::read_dir(dir.path()).unwrap_or_else(|e| panic!("read dir: {e}")) {
            let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("config.json.bak.") {
                backup_count += 1;
            }
        }
        assert!(backup_count >= 1);
    }
}
