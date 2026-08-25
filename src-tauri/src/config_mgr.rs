use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::security::SecurityError;

/// 插件级配置管理(~/.plugkit/configs/{plugin_id}/settings.json,key-value 字符串)。
/// 全局设置(下载镜像等)已移除——镜像改为内置候选池 + 自动降级,不再需要用户配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub values: HashMap<String, String>,
}

pub struct ConfigMgr {
    configs_root: PathBuf,
}

impl ConfigMgr {
    pub fn new() -> Self {
        Self {
            configs_root: crate::paths::configs_dir(),
        }
    }

    /// 校验 plugin_id 只含安全字符(字母/数字/下划线/连字符)。
    /// 拒绝路径分隔符与 `..`,防止插件 id 被用于路径穿越读写任意目录。
    fn validate_plugin_id(plugin_id: &str) -> Result<(), SecurityError> {
        if plugin_id.is_empty()
            || plugin_id.contains("..")
            || plugin_id.contains('/')
            || plugin_id.contains('\\')
            || !plugin_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(SecurityError::PathOutsideRoot(format!(
                "Invalid plugin id: {:?}",
                plugin_id
            )));
        }
        Ok(())
    }

    pub fn get(&self, plugin_id: &str, key: &str) -> Result<Option<String>, SecurityError> {
        Self::validate_plugin_id(plugin_id)?;
        let config_dir = self.configs_root.join(plugin_id);
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
        Self::validate_plugin_id(plugin_id)?;
        let config_dir = self.configs_root.join(plugin_id);
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
