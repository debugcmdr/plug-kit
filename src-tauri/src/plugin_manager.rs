use crate::manifest_model::Manifest;
use crate::security::SecurityError;
use std::fs;
use std::path::PathBuf;
use flate2::read::GzDecoder;
use tar::Archive;
use zip::ZipArchive;
use std::io::Cursor;

pub struct PluginManager {
    plugins_dir: PathBuf,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins_dir: dirs::home_dir()
                .map(|d| d.join(".plugkit/plugins"))
                .unwrap_or_default(),
        }
    }

    pub async fn install(&self, plugin_id: &str) -> Result<crate::InstallResult, String> {
        // 1. Resolve binary URL + sha256 from market manifest (or local reinstall).
        let market = crate::manifest_fetcher::fetch_market_manifests()
            .await?
            .into_iter()
            .find(|m| m.id == plugin_id);

        // 版本守卫:已安装版本 >= 市场版本时拒绝重装(防降级/防无谓覆盖)。
        if let Some(existing) = self.get_manifest(plugin_id).await? {
            if let Some(m) = &market {
                let existing_ver = semver::Version::parse(&existing.version)
                    .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                let market_ver = semver::Version::parse(&m.version)
                    .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                if existing_ver >= market_ver {
                    return Ok(crate::InstallResult {
                        success: false,
                        plugin_id: plugin_id.to_string(),
                        message: format!(
                            "已安装 {} v{}，市场最新为 v{}，无需重装（如需修复请先卸载）",
                            plugin_id, existing.version, m.version
                        ),
                        code: Some("ALREADY_LATEST".into()),
                    });
                }
            }
        }

        let (binary_url, sha256, signature) = match &market {
            Some(m) => {
                let platform = current_platform();
                let url = m.binary_url.replace("{platform}", &platform);
                (url, m.sha256.clone(), m.signature.clone())
            }
            None => {
                // Local reinstall (plugin already on disk): reuse its manifest.
                let manifest = self.fetch_manifest(plugin_id).await?;
                let url = manifest.binary_url.clone()
                    .ok_or_else(|| "本地插件未配置 binaryUrl,无法重装".to_string())?;
                (url, manifest.expected_sha256.clone(), None)
            }
        };

        // 2. Download + verify SHA256 (with mirror fallback).
        let tmp_dir = dirs::home_dir()
            .map(|d| d.join(".plugkit/tmp").join(plugin_id))
            .unwrap_or_default();
        // Clean any stale tmp dir from a previous failed install.
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)
            .map_err(|e| format!("Create tmp dir: {}", e))?;

        let bytes = crate::manifest_fetcher::download_with_fallback(
            &binary_url,
            sha256.as_deref(),
        ).await?;

        // 插件签名校验(可选):配置了官方公钥且清单带签名时强制执行。
        if let Some(pub_key) = crate::security::OFFICIAL_PUBLIC_KEY {
            if let Some(sig) = &signature {
                crate::security::verify_plugin_signature(&bytes, sig, pub_key)
                    .map_err(|e| format!("插件签名校验失败({}): {}", plugin_id, e))?;
                log::info!("Plugin '{}' signature verified", plugin_id);
            } else {
                log::warn!("Plugin '{}' has no signature but a trust anchor is configured", plugin_id);
                return Ok(crate::InstallResult {
                    success: false,
                    plugin_id: plugin_id.to_string(),
                    message: "此插件未签名,已拒绝安装(主程序已配置官方签名公钥)".to_string(),
                    code: Some("UNSIGNED_PLUGIN".into()),
                });
            }
        }

        // 3. Extract (Zip-Slip protected) + validate paths.
        self.extract_archive(&bytes, &tmp_dir, &binary_url)?;
        self.validate_extracted_paths(&tmp_dir).map_err(|e| e.to_string())?;

        // 4. Validate bridge version + minAppVersion (against the plugin's own manifest).
        let manifest_path = tmp_dir.join("manifest.json");
        if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("Read extracted manifest: {}", e))?;
            let manifest: crate::manifest_model::Manifest =
                serde_json::from_str(&content)
                    .map_err(|e| format!("Parse extracted manifest: {}", e))?;
            let current = crate::bridge_version::CURRENT_BRIDGE_VERSION;
            if let Err(_) = crate::bridge_version::check_bridge_version(&manifest.bridge_version, current) {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Ok(crate::InstallResult {
                    success: false,
                    plugin_id: plugin_id.to_string(),
                    message: format!("Bridge version incompatible: requires {}, has {}",
                        manifest.bridge_version, current),
                    code: Some("BRIDGE_INCOMPATIBLE".into()),
                });
            }
            // 主程序过旧时拒绝安装,提示升级主程序。
            if let Some(min_app) = &manifest.min_app_version {
                if let Err(_) = crate::bridge_version::check_bridge_version(
                    &format!(">={}", min_app),
                    env!("CARGO_PKG_VERSION"),
                ) {
                    let _ = fs::remove_dir_all(&tmp_dir);
                    return Ok(crate::InstallResult {
                        success: false,
                        plugin_id: plugin_id.to_string(),
                        message: format!(
                            "此插件要求主程序 >= v{}，当前为 v{}，请先升级 PlugKit",
                            min_app,
                            env!("CARGO_PKG_VERSION")
                        ),
                        code: Some("MIN_APP_VERSION".into()),
                    });
                }
            }
        }

        // 5. Move to plugins dir (atomic swap): 旧目录先挪到 .old,新目录落位后再删旧。
        //    修复:原先先 remove_dir_all(dest) 再 rename,rename 失败会丢失旧版本。
        let dest_dir = self.plugins_dir.join(plugin_id);
        let dest_old = self.plugins_dir.join(format!("{}.old", plugin_id));
        let _ = fs::remove_dir_all(&dest_old);
        if dest_dir.exists() {
            fs::rename(&dest_dir, &dest_old)
                .map_err(|e| format!("Backup old plugin: {}", e))?;
        }
        if let Err(e) = fs::rename(&tmp_dir, &dest_dir) {
            // 落位失败 → 回滚旧版本
            let _ = fs::rename(&dest_old, &dest_dir);
            return Err(format!("Move plugin: {}", e));
        }
        let _ = fs::remove_dir_all(&dest_old);

        // 6. Install shared dependencies declared in manifest (ffmpeg/yt-dlp etc.).
        self.install_dependencies(plugin_id).await;

        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' installed successfully", plugin_id),
            code: Some("INSTALL_OK".into()),
        })
    }

    pub async fn uninstall(&self, plugin_id: &str) -> Result<crate::InstallResult, String> {
        let plugin_dir = self.plugins_dir.join(plugin_id);
        if !plugin_dir.exists() {
            return Ok(crate::InstallResult {
                success: false,
                plugin_id: plugin_id.to_string(),
                message: format!("Plugin '{}' not found", plugin_id),
                code: Some("NOT_FOUND".into()),
            });
        }

        // 1. Decrement shared-dependency refcounts BEFORE deleting (so refs tracking
        //    works). Dependencies still referenced by other plugins are preserved.
        let dc = crate::dependency_cache::DependencyCache::new();
        if let Ok(Some(manifest)) = self.get_manifest(plugin_id).await {
            if let Some(deps) = &manifest.dependencies {
                let platform = crate::dep_manifest::current_platform();
                for (name, _) in deps {
                    let _ = dc.decrement_refcount(name, "latest", &platform, plugin_id).await;
                }
            }
        }

        // 2. Remove the plugin directory.
        fs::remove_dir_all(&plugin_dir)
            .map_err(|e| format!("Remove plugin: {}", e))?;

        // 3. Clean up now-orphaned dependencies (refcount == 0).
        let _ = dc.clean_orphans().await;

        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' uninstalled", plugin_id),
            code: Some("UNINSTALL_OK".into()),
        })
    }

    pub async fn list(&self) -> Result<Vec<crate::PluginInfo>, String> {
        let mut plugins = Vec::new();
        
        // Scan installed plugins
        if self.plugins_dir.exists() {
            for entry in fs::read_dir(&self.plugins_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let plugin_id = entry.file_name().to_string_lossy().to_string();
                let manifest_path = entry.path().join("manifest.json");
                
                if manifest_path.exists() {
                    let content = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
                    let manifest: Manifest = serde_json::from_str(&content)
                        .map_err(|e| e.to_string())?;
                    
                    plugins.push(crate::PluginInfo {
                        id: plugin_id,
                        name: manifest.name,
                        version: manifest.version.clone(),
                        description: manifest.description,
                        is_installed: true,
                        installed_version: Some(manifest.version),
                    });
                }
            }
        }
        
        // TODO: Fetch from plugin marketplace for uninstalled plugins
        
        Ok(plugins)
    }

    pub async fn get_manifest(&self, plugin_id: &str) -> Result<Option<Manifest>, String> {
        let manifest_path = self.plugins_dir.join(plugin_id).join("manifest.json");
        if !manifest_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| e.to_string())?;
        let manifest: Manifest = serde_json::from_str(&content)
            .map_err(|e| e.to_string())?;
        Ok(Some(manifest))
    }

    async fn fetch_manifest(&self, plugin_id: &str) -> Result<Manifest, String> {
        // For now, fetch from local or return error
        // In production, this would fetch from marketplace
        let manifest_path = self.plugins_dir.join(plugin_id).join("manifest.json");
        if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())?;
            return serde_json::from_str(&content).map_err(|e| e.to_string());
        }
        Err(format!("Plugin '{}' not found", plugin_id))
    }

    /// Extract an archive (zip / tar.gz) into dest. The archive bytes were already
    /// downloaded and SHA256-verified by the caller.
    fn extract_archive(&self, bytes: &[u8], dest: &PathBuf, source_name: &str) -> Result<(), String> {
        if source_name.ends_with(".zip") {
            self.extract_zip(bytes, dest)
        } else if source_name.ends_with(".tar.gz") || source_name.ends_with(".tgz") {
            self.extract_tar_gz(bytes, dest)
        } else {
            Err("Unsupported archive format".to_string())
        }
    }

    fn extract_zip(&self, bytes: &[u8], dest: &PathBuf) -> Result<(), String> {
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| format!("Invalid zip: {}", e))?;

        // Determine a single common top-level directory to strip (many archives
        // wrap their contents in a folder, e.g. `convert/manifest.json`). If all
        // entries share the same first path component, strip it so the plugin
        // root ends up directly under `dest`.
        let strip_prefix = common_top_dir(&mut archive);
        // 需要带尾斜杠的 prefix 才能正确剥离 `convert/manifest.json` -> `manifest.json`
        let strip_with_slash = strip_prefix.as_ref().map(|p| format!("{}/", p));

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| e.to_string())?;

            let raw_name = file.name().trim_end_matches('/');
            let entry_rel = match &strip_with_slash {
                Some(prefix) => raw_name.strip_prefix(prefix.as_str()).unwrap_or(raw_name),
                None => raw_name,
            };
            // Defensive: reject any residual traversal / absolute in the name.
            if entry_rel.starts_with('/') || entry_rel.split('/').any(|c| c == "..") {
                return Err(format!("Zip-Slip: unsafe entry '{}'", file.name()));
            }

            let outpath = dest.join(entry_rel);

            // Validate path (Zip-Slip prevention).
            crate::security::validate_unzip_path(dest, &outpath)
                .map_err(|e| e.to_string())?;

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)
                    .map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| e.to_string())?;
                }
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn extract_tar_gz(&self, bytes: &[u8], dest: &PathBuf) -> Result<(), String> {
        let cursor = Cursor::new(bytes);
        let decoder = GzDecoder::new(cursor);
        let mut archive = Archive::new(decoder);
        
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let outpath = dest.join(entry.path().map_err(|e| e.to_string())?);
            
            // Validate path
            crate::security::validate_unzip_path(dest, &outpath)
                .map_err(|e| e.to_string())?;
            
            entry.unpack(&outpath)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn validate_extracted_paths(&self, base: &PathBuf) -> Result<(), SecurityError> {
        for entry in walkdir::WalkDir::new(base) {
            let entry = entry.map_err(|e| SecurityError::PathOutsideRoot(e.to_string()))?;
            let path = entry.path();
            crate::security::validate_unzip_path(base, path)?;
        }
        Ok(())
    }    /// Install any shared dependencies declared in the plugin manifest.
    /// Shared deps (ffmpeg/yt-dlp) are cached in ~/.plugkit/cache/deps and injected
    /// into the subprocess via PLUGKIT_FFMPEG / PLUGKIT_YTDLP env vars at runtime.
    async fn install_dependencies(&self, plugin_id: &str) {
        let Ok(Some(manifest)) = self.get_manifest(plugin_id).await else {
            return;
        };
        let Some(deps) = &manifest.dependencies else {
            return;
        };
        let dc = crate::dependency_cache::DependencyCache::new();
        for (name, _constraint) in deps {
            if let Err(e) = dc.ensure_dependency(name, plugin_id).await {
                // pinned 版本下载失败 → 回退缓存中任意已存在版本,保证插件可用。
                let platform = crate::dep_manifest::current_platform();
                if let Some(cached) = dc.find_cached_any_version(name, &platform) {
                    log::warn!(
                        "Install dependency {} for {}: {} (fallback to cached {})",
                        name, plugin_id, e, cached.display()
                    );
                } else {
                    log::error!("Install dependency {} for {} failed: {}", name, plugin_id, e);
                }
            }
        }
    }
}

/// If every entry in the archive shares the same first path component (a common
/// top-level folder, e.g. `convert/manifest.json` + `convert/tool/...`), return
/// that prefix so it can be stripped during extraction. Returns None when there
/// is no single common top dir (files at archive root, or mixed roots).
fn common_top_dir(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Option<String> {
    let mut common: Option<String> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).ok()?;
        let name = entry.name().trim_end_matches('/');
        if name.is_empty() {
            continue;
        }
        let top = name.split('/').next().unwrap_or("");
        // __MACOSX 是 macOS zip 的元数据目录,跳过
        if top == "__MACOSX" {
            continue;
        }
        match &common {
            None => common = Some(top.to_string()),
            Some(existing) if existing == top => {}
            Some(_) => return None,
        }
    }
    common
}

/// Current platform string used for `{platform}` placeholder replacement.
fn current_platform() -> String {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64".to_string()
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64".to_string()
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64".to_string()
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64".to_string()
    } else {
        "unknown".to_string()
    }
}
