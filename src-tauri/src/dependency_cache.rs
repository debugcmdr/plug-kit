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
            &spec.version,
            &platform,
            &spec.url,
            &spec.sha256,
            spec.checksum_url.as_deref(),
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
    /// `checksum_url` 用于 sha256 缺失时从官方校验和文件动态校验(如 yt-dlp)。
    pub async fn install_dependency(
        &self,
        name: &str,
        version: &str,
        platform: &str,
        url: &str,
        sha256: &str,
        checksum_url: Option<&str>,
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
        self.download_dependency(url, &dest_dir, sha256, checksum_url, binary_name).await?;
        
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

    /// 下载依赖并校验:原始 URL + 各镜像(GitHub 源)串行降级,
    /// 失败的镜像进入会话黑名单,不再重复尝试。
    async fn download_dependency(
        &self,
        url: &str,
        dest: &Path,
        sha256: &str,
        checksum_url: Option<&str>,
        binary_name: &str,
    ) -> Result<(), String> {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Create dest dir: {}", e))?;

        let mut sources: Vec<String> = vec![url.to_string()];
        // 镜像仅对 GitHub 托管的资产生效(与插件包下载一致:GitHub 是国内被墙主因)。
        // 注意:镜像前缀保留尾部斜杠(mirror 形如 https://ghfast.top/),拼接 = 前缀 + 完整 URL。
        if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
            for m in crate::manifest_fetcher::enabled_mirrors() {
                sources.push(format!("{}{}", m, url));
            }
        }

        let mut last_err = String::from("No source attempted");
        for src in sources {
            if src != url && crate::manifest_fetcher::is_dead_mirror(&src) {
                continue;
            }
            match self.download_and_verify(&src, dest, sha256, checksum_url, url, binary_name).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if src != url {
                        crate::manifest_fetcher::mark_dead_mirror(&src);
                    }
                    last_err = format!("{}: {}", src, e);
                }
            }
        }
        Err(format!("All download sources failed: {}", last_err))
    }

    /// 单源下载 + 完整性校验 + 落盘。校验失败视为该源不可用(交由上层降级)。
    async fn download_and_verify(
        &self,
        src: &str,
        dest: &Path,
        sha256: &str,
        checksum_url: Option<&str>,
        original_url: &str,
        binary_name: &str,
    ) -> Result<(), String> {
        let resp = HTTP_CLIENT.get(src)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("Download failed with status {}", resp.status()));
        }
        let bytes = resp.bytes()
            .await
            .map_err(|e| format!("Read bytes failed: {}", e))?;

        // Verify SHA256:
        // 1) 固定 sha256(若配置);
        // 2) 否则从官方校验和文件(SHA2-256SUMS)动态校验 —— yt-dlp pinned 后完整性可验证。
        if !sha256.is_empty() {
            crate::security::validate_sha256(&bytes, sha256)
                .map_err(|e| e.to_string())?;
        } else if let Some(cu) = checksum_url {
            let asset_name = original_url.rsplit('/').next().unwrap_or(binary_name);
            let checksum = Self::fetch_checksum_for(cu, asset_name).await?;
            crate::security::validate_sha256(&bytes, &checksum)
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

    /// 从官方校验和文件(格式:`<hex>  <asset_name>`)解析指定资产名的 sha256。
    /// checksum 文件同样走镜像降级(它也是 GitHub 资产)。
    async fn fetch_checksum_for(checksum_url: &str, asset_name: &str) -> Result<String, String> {
        let mut sources: Vec<String> = vec![checksum_url.to_string()];
        if checksum_url.starts_with("https://github.com/") || checksum_url.starts_with("http://github.com/") {
            for m in crate::manifest_fetcher::enabled_mirrors() {
                sources.push(format!("{}{}", m, checksum_url));
            }
        }

        let mut last_err = String::from("No source attempted");
        for src in sources {
            if src != checksum_url && crate::manifest_fetcher::is_dead_mirror(&src) {
                continue;
            }
            match HTTP_CLIENT.get(&src).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.text().await {
                        Ok(text) => {
                            for line in text.lines() {
                                let parts: Vec<&str> = line.split_whitespace().collect();
                                if parts.len() >= 2 && parts[1] == asset_name {
                                    return Ok(parts[0].to_string());
                                }
                            }
                            // 文件拿到了但没有该资产条目 → 无需再试其他源
                            return Err(format!(
                                "Checksum file has no entry for asset '{}'",
                                asset_name
                            ));
                        }
                        Err(e) => last_err = format!("{}: read failed: {}", src, e),
                    }
                }
                Ok(resp) => last_err = format!("{}: HTTP {}", src, resp.status()),
                Err(e) => last_err = format!("{}: {}", src, e),
            }
            if src != checksum_url {
                crate::manifest_fetcher::mark_dead_mirror(&src);
            }
        }
        Err(format!("All checksum sources failed: {}", last_err))
    }

    /// 找缓存中「任意已存在二进制」的版本(不要求 pinned 版本)。
    /// 用途:pinned 版本下载失败(如 GitHub 被墙)时回退到历史缓存版本,保证依赖仍可用。
    /// 先查 registry 登记(取 last_accessed 最新),再直接扫描缓存目录兜底
    /// (未登记的历史缓存,如旧版 latest 目录,也能被回退使用)。
    pub fn find_cached_any_version(&self, name: &str, platform: &str) -> Option<PathBuf> {
        if let Some(p) = self.find_from_registry(name, platform) {
            return Some(p);
        }
        // 兜底:扫描 {cache}/deps/{name}/*/{platform}/* 找二进制(按 mtime 最新优先)。
        let root = self.cache_dir.join(name);
        let Ok(rd) = fs::read_dir(&root) else { return None };
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for ver in rd.flatten() {
            if !ver.path().is_dir() { continue; }
            let plat_dir = ver.path().join(platform);
            let Ok(prd) = fs::read_dir(&plat_dir) else { continue };
            for f in prd.flatten() {
                let p = f.path();
                let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if fname.starts_with('.') || fname.starts_with("download.tmp") || !p.is_file() {
                    continue;
                }
                let mtime = p.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                if best.as_ref().map_or(true, |(t, _)| mtime > *t) {
                    best = Some((mtime, p));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// 从 registry 登记中找任意已落盘二进制的版本(last_accessed 最新优先)。
    fn find_from_registry(&self, name: &str, platform: &str) -> Option<PathBuf> {
        let registry = Registry::load_or_new(&self.registry_path).ok()?;
        let entries = match name {
            "ffmpeg" => &registry.ffmpeg,
            "yt-dlp" => &registry.yt_dlp,
            _ => &registry.other,
        };
        let mut best: Option<(i64, PathBuf)> = None;
        for (_, platforms) in entries {
            let Some(entry) = platforms.get(platform) else { continue };
            let dir = self.cache_dir.join(&entry.path).join(platform);
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if fname.starts_with('.') || fname.starts_with("download.tmp") || !p.is_file() {
                    continue;
                }
                if best.as_ref().map_or(true, |(ts, _)| entry.last_accessed > *ts) {
                    best = Some((entry.last_accessed, p));
                }
            }
        }
        best.map(|(_, p)| p)
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
