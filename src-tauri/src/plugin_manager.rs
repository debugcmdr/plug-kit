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

        let (binary_url, sha256) = match market {
            Some(m) => {
                let platform = current_platform();
                let url = m.binary_url.replace("{platform}", &platform);
                (url, m.sha256.as_deref().map(String::from))
            }
            None => {
                // Local reinstall (plugin already on disk): reuse its manifest.
                let manifest = self.fetch_manifest(plugin_id).await?;
                (manifest.binary_url.clone(), manifest.expected_sha256.clone())
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

        // 3. Extract (Zip-Slip protected) + validate paths.
        self.extract_archive(&bytes, &tmp_dir, &binary_url)?;
        self.validate_extracted_paths(&tmp_dir).map_err(|e| e.to_string())?;

        // 4. Validate bridge version (against the plugin's own manifest).
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
                });
            }
        }

        // 5. Move to plugins dir (atomic-ish).
        let dest_dir = self.plugins_dir.join(plugin_id);
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir).map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp_dir, &dest_dir)
            .map_err(|e| format!("Move plugin: {}", e))?;

        // 6. Install shared dependencies declared in manifest (ffmpeg/yt-dlp etc.).
        self.install_dependencies(plugin_id).await;

        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' installed successfully", plugin_id),
        })
    }

    pub async fn uninstall(&self, plugin_id: &str) -> Result<crate::InstallResult, String> {
        let plugin_dir = self.plugins_dir.join(plugin_id);
        if !plugin_dir.exists() {
            return Ok(crate::InstallResult {
                success: false,
                plugin_id: plugin_id.to_string(),
                message: format!("Plugin '{}' not found", plugin_id),
            });
        }
        
        fs::remove_dir_all(&plugin_dir)
            .map_err(|e| format!("Remove plugin: {}", e))?;
        
        // Decrement refcount in dependency cache
        let dc = crate::dependency_cache::DependencyCache::new();
        dc.clean_orphans().await.ok();
        
        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' uninstalled", plugin_id),
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
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| e.to_string())?;
            
            let outpath = dest.join(file.name());
            
            // Validate path (Zip-Slip prevention)
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
    }

    /// Install any shared dependencies declared in the plugin manifest.
    /// Shared deps (ffmpeg/yt-dlp) are cached in ~/.plugkit/cache/deps and injected
    /// into the subprocess via PLUGKIT_FFMPEG / PLUGKIT_YTDLP env vars at runtime.
    async fn install_dependencies(&self, plugin_id: &str) {
        let Ok(Some(manifest)) = self.get_manifest(plugin_id).await else {
            return;
        };
        let Some(deps) = &manifest.dependencies else {
            return;
        };
        for (name, constraint) in deps {
            // v0.2+: resolve version from semver constraint; for now log the intent.
            log::info!("Plugin {} needs dependency {} ({}) — dependency fetching in v0.2", plugin_id, name, constraint);
        }
    }
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
