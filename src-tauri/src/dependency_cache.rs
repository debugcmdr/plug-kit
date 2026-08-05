use crate::registry::{Registry, DepEntry};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use fs2::FileExt;

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

    pub async fn install_dependency(
        &self,
        name: &str,
        version: &str,
        platform: &str,
        url: &str,
        sha256: &str,
        ref_plugins: &[&str],
    ) -> Result<PathBuf, String> {
        let _lock = self.acquire_lock()?;
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;
        
        let key = format!("{}/{}", name, version);
        let dest_dir = self.cache_dir.join(&key).join(platform);
        
        // Check if already exists
        if dest_dir.exists() {
            // Increment refcount
            self.increment_refcount(&mut registry, name, version, platform, ref_plugins)?;
            return Ok(dest_dir);
        }
        
        // Download
        self.download_dependency(url, &dest_dir, sha256).await?;
        
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
    ) -> Result<(), String> {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Create dest dir: {}", e))?;
        
        let resp = reqwest::get(url)
            .await
            .map_err(|e| format!("Download failed: {}", e))?;
        
        if !resp.status().is_success() {
            return Err(format!("Download failed with status {}", resp.status()));
        }
        
        let bytes = resp.bytes()
            .await
            .map_err(|e| format!("Read bytes failed: {}", e))?;
        
        // Verify SHA256
        crate::security::validate_sha256(&bytes, sha256)
            .map_err(|e| e.to_string())?;
        
        // Write to temp then rename
        let tmp = dest.join("download.tmp");
        fs::write(&tmp, bytes)
            .map_err(|e| format!("Write file: {}", e))?;
        fs::rename(&tmp, dest.join("binary"))
            .map_err(|e| format!("Rename file: {}", e))?;
        
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
