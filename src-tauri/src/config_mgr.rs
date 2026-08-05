use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::security::SecurityError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_mirrors: Vec<MirrorConfig>,
    pub system_proxy: Option<ProxyConfig>,
    pub disk_quota_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorConfig {
    pub name: String,
    pub prefix: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_mirrors: vec![
                MirrorConfig { name: "ghproxy".into(), prefix: "https://ghproxy.com/".into(), enabled: true },
                MirrorConfig { name: "gh-proxy".into(), prefix: "https://gh-proxy.com/".into(), enabled: true },
                MirrorConfig { name: "custom".into(), prefix: String::new(), enabled: false },
            ],
            system_proxy: None,
            disk_quota_mb: 500,
        }
    }
}

pub struct ConfigMgr {
    settings_path: PathBuf,
}

impl ConfigMgr {
    pub fn new() -> Self {
        Self {
            settings_path: dirs::home_dir()
                .unwrap_or_default()
                .join(".plugkit")
                .join("settings.json"),
        }
    }

    pub fn get_settings(&self) -> Result<serde_json::Value, SecurityError> {
        let content = if self.settings_path.exists() {
            fs::read_to_string(&self.settings_path).unwrap_or_default()
        } else {
            serde_json::to_string_pretty(&Settings::default()).unwrap_or_default()
        };
        serde_json::from_str(&content).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Parse settings: {}", e))
        })
    }

    pub fn set_settings(&self, settings: serde_json::Value) -> Result<(), SecurityError> {
        let settings_dir = self.settings_path.parent().unwrap();
        fs::create_dir_all(settings_dir).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Create settings dir: {}", e))
        })?;
        let tmp = self.settings_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&settings).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Serialize settings: {}", e))
        })?;
        fs::write(&tmp, content).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Write settings: {}", e))
        })?;
        fs::rename(&tmp, &self.settings_path).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Rename settings: {}", e))
        })
    }

    pub fn get(&self, plugin_id: &str, key: &str) -> Result<Option<String>, SecurityError> {
        let config_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".plugkit")
            .join("configs")
            .join(plugin_id);
        let config_file = config_dir.join("settings.json");
        if !config_file.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&config_file).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Read config: {}", e))
        })?;
        let map: HashMap<String, String> = serde_json::from_str(&content).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Parse config: {}", e))
        })?;
        Ok(map.get(key).cloned())
    }

    pub fn set(&self, plugin_id: &str, key: &str, value: &str) -> Result<(), SecurityError> {
        let config_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".plugkit")
            .join("configs")
            .join(plugin_id);
        fs::create_dir_all(&config_dir).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Create config dir: {}", e))
        })?;
        let config_file = config_dir.join("settings.json");
        let mut map: HashMap<String, String> = if config_file.exists() {
            let content = fs::read_to_string(&config_file).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };
        map.insert(key.to_string(), value.to_string());
        let tmp = config_file.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&map).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Serialize config: {}", e))
        })?;
        fs::write(&tmp, content).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Write config: {}", e))
        })?;
        fs::rename(&tmp, &config_file).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Rename config: {}", e))
        })
    }
}
