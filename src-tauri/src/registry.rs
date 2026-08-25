use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use crate::security::SecurityError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    #[serde(rename = "_v")]
    pub version: u32,
    #[serde(default)]
    pub ffmpeg: HashMap<String, HashMap<String, DepEntry>>,
    #[serde(default)]
    pub yt_dlp: HashMap<String, HashMap<String, DepEntry>>,
    #[serde(default)]
    pub other: HashMap<String, HashMap<String, DepEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub refcount: u32,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub last_accessed: i64,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: 4,
            ffmpeg: HashMap::new(),
            yt_dlp: HashMap::new(),
            other: HashMap::new(),
        }
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_or_new(path: &Path) -> Result<Self, SecurityError> {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str::<Registry>(&content) {
                    Ok(reg) => Ok(reg),
                    Err(_) => Self::recover_from_backup(path),
                },
                Err(_) => Self::recover_from_backup(path),
            }
        } else {
            Ok(Self::new())
        }
    }

    fn recover_from_backup(path: &Path) -> Result<Self, SecurityError> {
        for i in 1..=3 {
            let backup = path.with_file_name(format!("registry.json.bak.{}", i));
            if backup.exists() {
                if let Ok(content) = fs::read_to_string(&backup) {
                    if let Ok(reg) = serde_json::from_str::<Registry>(&content) {
                        return Ok(reg);
                    }
                }
            }
        }
        Ok(Self::new())
    }

    pub fn save(&self, path: &Path) -> Result<(), SecurityError> {
        if path.exists() {
            for i in 1..=3 {
                let src = path.with_file_name(format!("registry.json.bak.{}", i));
                let dst = path.with_file_name(format!("registry.json.bak.{}", i + 1));
                if src.exists() {
                    let _ = fs::copy(&src, &dst);
                }
            }
            let _ = fs::copy(path, path.with_file_name("registry.json.bak.1"));
        }

        let tmp = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Serialize: {}", e))
        })?;
        let mut file = fs::File::create(&tmp).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Create tmp: {}", e))
        })?;
        file.write_all(content.as_bytes()).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Write: {}", e))
        })?;
        fs::rename(&tmp, path).map_err(|e| {
            SecurityError::PathOutsideRoot(format!("Rename: {}", e))
        })
    }

    pub fn total_size(&self) -> u64 {
        let mut total = 0u64;
        for versions in self.ffmpeg.values().chain(self.yt_dlp.values()).chain(self.other.values()) {
            for entries in versions.values() {
                total += entries.size;
            }
        }
        total
    }

    /// Total number of cached dependency entries (across all names/versions/platforms).
    /// registry 结构为 name → version → platform:第一层 values() 是各版本的
    /// 平台映射,`versions.len()` 即该版本下的平台条目数,汇总即总条目数。
    pub fn entry_count(&self) -> usize {
        let mut count = 0usize;
        for versions in self.ffmpeg.values().chain(self.yt_dlp.values()).chain(self.other.values()) {
            count += versions.len();
        }
        count
    }
}
