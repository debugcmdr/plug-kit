use crate::registry::{Registry, DepEntry};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use fs2::FileExt;
use once_cell::sync::Lazy;

/// 共享 HTTP 客户端(依赖下载用，120s 超时)。
pub static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("reqwest client build failed")
});

pub struct DependencyCache {
    cache_dir: PathBuf,
    registry_path: PathBuf,
}

impl DependencyCache {
    pub fn new() -> Self {
        let cache_dir = dirs::home_dir()
            .map(|d| d.join(".plugkit/cache/deps"))
            .unwrap_or_default();
        Self {
            cache_dir: cache_dir.clone(),
            registry_path: cache_dir.join("registry.json"),
        }
    }

    pub fn stats(&self) -> Result<serde_json::Value, String> {
        let registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;
        let total_size = registry.total_size();
        let entry_count = self.count_entries()?;
        
        Ok(serde_json::json!({
            "total_size_bytes": total_size,
            "total_size_mb": total_size as f64 / 1_000_000.0,
            "entry_count": entry_count,
            "quota_mb": registry.disk_quota_mb
        }))
    }

    pub async fn clean_orphans(&self) -> Result<serde_json::Value, String> {
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;
        
        let before_size = registry.total_size();
        let before_count = self.count_entries()?;
        
        // Remove entries with refcount = 0
        self.remove_zero_ref_entries(&mut registry)?;
        
        registry.save(&self.registry_path)
            .map_err(|e| e.to_string())?;
        
        let after_size = registry.total_size();
        let freed = before_size - after_size;
        
        Ok(serde_json::json!({
            "freed_bytes": freed,
            "freed_mb": freed as f64 / 1_000_000.0,
            "entries_removed": before_count - self.count_entries()?
        }))
    }

    fn remove_zero_ref_entries(&self, registry: &mut Registry) -> Result<(), String> {
        // Check ffmpeg
        let refs = &mut registry.ffmpeg;
        refs.retain(|_ver, platforms| {
            platforms.retain(|_plat, entry| entry.refcount > 0);
            !platforms.is_empty()
        });
        // Check yt-dlp
        let refs = &mut registry.yt_dlp;
        refs.retain(|_ver, platforms| {
            platforms.retain(|_plat, entry| entry.refcount > 0);
            !platforms.is_empty()
        });
        // Check other
        let refs = &mut registry.other;
        refs.retain(|_name, versions| {
            versions.retain(|_ver, entry| entry.refcount > 0);
            !versions.is_empty()
        });
        Ok(())
    }

    fn count_entries(&self) -> Result<usize, String> {
        let registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;
        Ok(registry.entry_count())
    }

    /// 安装共享依赖(name: ffmpeg/yt-dlp),自动解析下载源。
    /// 若系统 PATH 已可用则直接复用,不重复下载。
    /// 返回缓存中的可执行文件路径。
    pub async fn ensure_dependency(
        &self,
        name: &str,
        ref_plugin: &str,
    ) -> Result<PathBuf, String> {
        // 系统 PATH 已有 → 直接复用,不占用缓存
        if crate::dep_manifest::dep_available_on_path(name) {
            return Ok(PathBuf::from(name));
        }

        let spec = crate::dep_manifest::resolve_dep_spec(name)
            .ok_or_else(|| format!("Unknown shared dependency: {}", name))?;
        let platform = crate::dep_manifest::current_platform();
        let dest = self.install_dependency(
            &spec.name,
            "latest",
            &platform,
            &spec.url,
            &spec.sha256,
            &spec.binary_name,
            &[ref_plugin],
        ).await?;
        let exe = dest.join(&spec.binary_name);
        if !exe.exists() {
            return Err(format!("Dependency '{}' installed but binary not found at {:?}", name, exe));
        }
        Ok(exe)
    }

    /// 安装共享依赖(下载 + 引用计数 + registry)。若已缓存则仅递增 refcount。
    pub async fn install_dependency(
        &self,
        name: &str,
        version: &str,
        platform: &str,
        url: &str,
        sha256: &str,
        binary_name: &str,
        ref_plugins: &[&str],
    ) -> Result<PathBuf, String> {
        let _lock = self.acquire_lock()?;
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;

        let key = format!("{}/{}", name, version);
        let dest_dir = self.cache_dir.join(&key).join(platform);

        // Check if already exists (cache dir present => binary already there)
        if dest_dir.exists() && dest_dir.join(binary_name).exists() {
            // Increment refcount
            self.increment_refcount(&mut registry, name, version, platform, ref_plugins)?;
            return Ok(dest_dir);
        }

        // Download
        self.download_dependency(url, &dest_dir, sha256, binary_name).await?;
        
        // Get size
        let size = Self::dir_size(&dest_dir)?;
        
        // Add to registry
        let entry = DepEntry {
            path: key.clone(),
            sha256: sha256.to_string(),
            size,
            refcount: 1,
            refs: ref_plugins.iter().map(|s| s.to_string()).collect(),
            last_accessed: chrono::Utc::now().timestamp(),
        };
        
        self.add_to_registry(&mut registry, name, version, platform, entry)?;
        
        registry.save(&self.registry_path).map_err(|e| e.to_string())?;
        
        Ok(dest_dir)
    }

    fn acquire_lock(&self) -> Result<std::fs::File, String> {
        fs::create_dir_all(&self.cache_dir)
            .map_err(|e| format!("Create cache dir: {}", e))?;

        let lock_file = self.registry_path.with_extension("lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_file)
            .map_err(|e| format!("Open lock file: {}", e))?;

        file.lock_exclusive()
            .map_err(|e| format!("Lock file: {}", e))?;
        Ok(file)
    }

    fn increment_refcount(
        &self,
        registry: &mut Registry,
        name: &str,
        version: &str,
        platform: &str,
        ref_plugins: &[&str],
    ) -> Result<(), String> {
        let entries = self.get_mut_entries(registry, name)?;
        if let Some(platforms) = entries.get_mut(version) {
            if let Some(entry) = platforms.get_mut(platform) {
                entry.refcount += 1;
                entry.refs.extend(ref_plugins.iter().map(|s| s.to_string()));
                entry.last_accessed = chrono::Utc::now().timestamp();
            }
        }
        Ok(())
    }

    /// 卸载插件时递减引用计数。不为 0 则保留缓存;为 0 进入孤儿,
    /// 由 clean_orphans 清理。返回该依赖是否已无引用(可安全删除)。
    pub async fn decrement_refcount(
        &self,
        name: &str,
        version: &str,
        platform: &str,
        plugin_id: &str,
    ) -> Result<bool, String> {
        let _lock = self.acquire_lock()?;
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;

        let entries = self.get_mut_entries(&mut registry, name)?;
        let mut orphaned = false;
        if let Some(platforms) = entries.get_mut(version) {
            if let Some(entry) = platforms.get_mut(platform) {
                entry.refs.retain(|r| r != plugin_id);
                if entry.refcount > 0 {
                    entry.refcount -= 1;
                }
                // 该依赖不再被任何插件引用 → 孤儿,可清理
                if entry.refcount == 0 || entry.refs.is_empty() {
                    orphaned = true;
                }
            }
        }
        registry.save(&self.registry_path).map_err(|e| e.to_string())?;
        Ok(orphaned)
    }

    fn get_mut_entries<'a>(
        &'a self,
        registry: &'a mut Registry,
        name: &str,
    ) -> Result<&'a mut HashMap<String, HashMap<String, DepEntry>>, String> {
        if name == "ffmpeg" {
            Ok(&mut registry.ffmpeg)
        } else if name == "yt-dlp" {
            Ok(&mut registry.yt_dlp)
        } else {
            Ok(&mut registry.other)
        }
    }

    fn add_to_registry(
        &self,
        registry: &mut Registry,
        name: &str,
        version: &str,
        platform: &str,
        entry: DepEntry,
    ) -> Result<(), String> {
        let entries = self.get_mut_entries(registry, name)?;
        entries.entry(version.to_string())
            .or_insert_with(HashMap::new)
            .insert(platform.to_string(), entry);
        Ok(())
    }

    async fn download_dependency(
        &self,
        url: &str,
        dest: &Path,
        sha256: &str,
        binary_name: &str,
    ) -> Result<(), String> {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Create dest dir: {}", e))?;
        
        let resp = HTTP_CLIENT.get(url)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;
        
        if !resp.status().is_success() {
            return Err(format!("Download failed with status {}", resp.status()));
        }
        
        let bytes = resp.bytes()
            .await
            .map_err(|e| format!("Read bytes failed: {}", e))?;
        
        // Verify SHA256 (only if provided; yt-dlp official release has no pinned hash)
        if !sha256.is_empty() {
            crate::security::validate_sha256(&bytes, sha256)
                .map_err(|e| e.to_string())?;
        }

        // Write to temp then rename to the binary name
        let tmp = dest.join("download.tmp");
        fs::write(&tmp, bytes)
            .map_err(|e| format!("Write file: {}", e))?;
        fs::rename(&tmp, dest.join(binary_name))
            .map_err(|e| format!("Rename file: {}", e))?;

        // 可执行权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(dest.join(binary_name), fs::Permissions::from_mode(0o755));
        }

        Ok(())
    }

    fn dir_size(path: &Path) -> Result<u64, String> {
        let mut size = 0u64;
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().is_file() {
                size += entry.metadata().map_err(|e| e.to_string())?.len();
            }
        }
        Ok(size)
    }
}
