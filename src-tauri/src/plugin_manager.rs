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
                .map(|d| d.join(".multitool/plugins"))
                .unwrap_or_default(),
        }
    }

    pub async fn install(&self, plugin_id: &str) -> Result<crate::InstallResult, String> {
        // 1. Fetch manifest
        let manifest = self.fetch_manifest(plugin_id).await?;
        
        // 2. Validate bridge version
        let current = crate::bridge_version::CURRENT_BRIDGE_VERSION;
        if let Err(_) = crate::bridge_version::check_bridge_version(&manifest.bridge_version, current) {
            return Ok(crate::InstallResult {
                success: false,
                plugin_id: plugin_id.to_string(),
                message: format!("Bridge version incompatible: requires {}, has {}", 
                    manifest.bridge_version, current),
            });
        }
        
        // 3. Download and extract
        let tmp_dir = dirs::home_dir()
            .map(|d| d.join(".multitool/tmp").join(plugin_id))
            .unwrap_or_default();
        fs::create_dir_all(&tmp_dir)
            .map_err(|e| format!("Create tmp dir: {}", e))?;
        
        self.download_and_extract(&manifest.binary_url, &tmp_dir, manifest.expected_sha256.as_deref())
            .await?;
        
        // 4. Validate paths (Zip-Slip prevention)
        self.validate_extracted_paths(&tmp_dir).map_err(|e| e.to_string())?;
        
        // 5. Move to plugins dir
        let dest_dir = self.plugins_dir.join(plugin_id);
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir).map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp_dir, &dest_dir)
            .map_err(|e| format!("Move plugin: {}", e))?;
        
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

    async fn download_and_extract(
        &self,
        url: &str,
        dest: &PathBuf,
        expected_sha256: Option<&str>,
    ) -> Result<(), String> {
        let client = reqwest::Client::new();
        let bytes = client.get(url).send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("Read bytes failed: {}", e))?;
        
        // Verify SHA256 if provided
        if let Some(sha256) = expected_sha256 {
            crate::security::validate_sha256(&bytes, sha256)
                .map_err(|e| e.to_string())?;
        }
        
        // Detect archive type and extract
        if url.ends_with(".zip") {
            self.extract_zip(&bytes, dest)?;
        } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
            self.extract_tar_gz(&bytes, dest)?;
        } else {
            return Err("Unsupported archive format".to_string());
        }
        
        Ok(())
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
}
