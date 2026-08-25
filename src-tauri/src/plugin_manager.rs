use crate::manifest_model::Manifest;
use crate::security::SecurityError;
use std::fs;
use std::path::PathBuf;
use zip::ZipArchive;
use std::io::Cursor;

pub struct PluginManager {
    plugins_dir: PathBuf,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins_dir: crate::paths::plugins_dir(),
        }
    }

    pub async fn install(&self, plugin_id: &str) -> Result<crate::InstallResult, String> {
        // 1. Resolve binary URL + sha256 from market manifest (or local reinstall).
        //    市场拉取失败(离线/被墙)不阻断安装(R-14):降级走本地重装分支——
        //    若插件已安装,用其本地 manifest 的 binaryUrl 重装;未安装则报市场不可达。
        let market = match crate::manifest_fetcher::fetch_market_manifests().await {
            Ok(list) => list.into_iter().find(|m| m.id == plugin_id),
            Err(e) => {
                log::warn!("install '{}': market fetch failed ({}), fallback to local reinstall", plugin_id, e);
                None
            }
        };

        // 版本守卫:已安装版本 >= 市场版本时拒绝重装(防降级/防无谓覆盖)。
        // 版本解析失败(R-15):不能按 0.0.0 参与比较(会恒判"已是最新"而永久拒绝重装),
        // 解析失败即视为"需重装"放行——交给后续下载校验兜底。
        if let Some(existing) = self.get_manifest(plugin_id).await? {
            if let Some(m) = &market {
                let guard = semver::Version::parse(&existing.version)
                    .and_then(|ev| semver::Version::parse(&m.version).map(|mv| (ev, mv)))
                    .ok();
                if let Some((existing_ver, market_ver)) = guard {
                    if existing_ver >= market_ver {
                        return Ok(crate::InstallResult {
                            success: false,
                            plugin_id: plugin_id.to_string(),
                            message: format!(
                                "已安装 {} v{}，市场最新为 v{}，无需重装（如需修复请先卸载）",
                                plugin_id, existing.version, m.version
                            ),
                            code: Some("ALREADY_LATEST".into()),
                            warnings: vec![],
                        });
                    }
                }
            }
        }

        let (binary_url, sha256, signature) = match &market {
            Some(m) => {
                let platform = crate::dep_manifest::current_platform();
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
        let tmp_dir = crate::paths::tmp_dir().join(plugin_id);
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
                    warnings: vec![],
                });
            }
        }

        // 3. Extract (Zip-Slip protected) + validate paths.
        self.extract_archive(&bytes, &tmp_dir, &binary_url)?;
        self.validate_extracted_paths(&tmp_dir).map_err(|e| e.to_string())?;

        // 4. Validate the extracted plugin package:
        //    - manifest.json 必须存在(缺失即包损坏,拒绝安装)
        //    - bridgeVersion / minAppVersion 与主程序匹配
        //    - entry.executable 存在(错误前置:安装阶段校验,运行时无需盲扫回退)
        let manifest_path = tmp_dir.join("manifest.json");
        if !manifest_path.exists() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!("插件包缺少 manifest.json（{}）", plugin_id));
        }
        let content = fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Read extracted manifest: {}", e))?;
        let manifest: crate::manifest_model::Manifest =
            serde_json::from_str(&content)
                .map_err(|e| format!("Parse extracted manifest: {}", e))?;
        let current = crate::bridge_version::CURRENT_BRIDGE_VERSION;
        // 版本不匹配(Ok(false))与解析失败(Err)都必须拒绝——此前 `if let Err(_)`
        // 只捕获解析错误,Ok(false)被放行导致不兼容插件被安装(与 preinstall.rs 对照)。
        if !crate::bridge_version::check_bridge_version(&manifest.bridge_version, current)
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(crate::InstallResult {
                success: false,
                plugin_id: plugin_id.to_string(),
                message: format!("Bridge version incompatible: requires {}, has {}",
                    manifest.bridge_version, current),
                code: Some("BRIDGE_INCOMPATIBLE".into()),
                warnings: vec![],
            });
        }
        // 主程序过旧时拒绝安装,提示升级主程序。
        if let Some(min_app) = &manifest.min_app_version {
            if !crate::bridge_version::check_bridge_version(
                &format!(">={}", min_app),
                env!("CARGO_PKG_VERSION"),
            )
            .unwrap_or(false)
            {
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
                    warnings: vec![],
                });
            }
        }
        // 入口可执行文件存在性校验(错误前置;tool_runner 运行时直接定位,不再盲扫)。
        let exe = tmp_dir.join(manifest.tool_dir()).join(&manifest.entry.executable);
        if !exe.is_file() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!(
                "插件缺少可执行文件 {}（{}）",
                manifest.entry.executable, plugin_id
            ));
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
        //    依赖安装失败不阻断安装，但警告随 InstallResult 透出（用户可见，而非仅日志）。
        let warnings = self.install_dependencies(plugin_id).await;

        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' installed successfully", plugin_id),
            code: Some("INSTALL_OK".into()),
            warnings,
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
                warnings: vec![],
            });
        }

        // 1. 先删插件目录：删除失败立即中止，共享依赖引用保持不变（状态一致）。
        //    (原实现先递减 refcount 再删目录——Windows 上运行中插件的目录删除会失败，
        //    导致"卸载失败但依赖已被清理"的不一致状态。)
        fs::remove_dir_all(&plugin_dir)
            .map_err(|e| format!("Remove plugin: {}", e))?;

        // 2. 递减共享依赖引用计数：按 plugin_id 扫描 registry 的 refs 递减
        //    （与安装时的递增严格对称；依赖仍被其他插件引用时保留）。
        //    失败不能静默(G-5):引用未递减会让依赖成为永久孤儿,占用磁盘。
        let dc = crate::dependency_cache::DependencyCache::new();
        if let Err(e) = dc.decrement_refcount_for_plugin(plugin_id).await {
            log::error!("Uninstall '{}': decrement dependency refcount failed: {}", plugin_id, e);
        }

        // 3. Clean up now-orphaned dependencies (refcount == 0).
        if let Err(e) = dc.clean_orphans().await {
            log::error!("Uninstall '{}': clean orphan dependencies failed: {}", plugin_id, e);
        }

        Ok(crate::InstallResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            message: format!("Plugin '{}' uninstalled", plugin_id),
            code: Some("UNINSTALL_OK".into()),
            warnings: vec![],
        })
    }

    pub async fn list(&self) -> Result<Vec<crate::PluginInfo>, String> {
        let mut plugins = Vec::new();
        
        // Scan installed plugins
        if self.plugins_dir.exists() {
            for entry in fs::read_dir(&self.plugins_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let plugin_id = entry.file_name().to_string_lossy().to_string();
                // 原子换装崩溃可能残留 {id}.old 目录:不是插件,跳过(R-16)
                if plugin_id.ends_with(".old") {
                    continue;
                }
                let manifest_path = entry.path().join("manifest.json");
                
                // 单个插件 manifest 损坏不应拖垮整个列表：跳过 + 记录日志。
                // (原实现 `?` 传播会让侧边栏/市场面板整体加载失败。)
                if manifest_path.exists() {
                    let content = match fs::read_to_string(&manifest_path) {
                        Ok(c) => c,
                        Err(e) => {
                            log::warn!("List plugin '{}': read manifest failed ({})", plugin_id, e);
                            continue;
                        }
                    };
                    let manifest: Manifest = match serde_json::from_str(&content) {
                        Ok(m) => m,
                        Err(e) => {
                            log::warn!("List plugin '{}': invalid manifest ({}), skipped", plugin_id, e);
                            continue;
                        }
                    };
                    
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

    /// Extract an archive into dest. 插件包由主仓库打包脚本统一产出 zip
    /// (确定性构建,sha256 可复现),因此只支持 .zip——不再为假想的外部
    /// 来源保留 tar.gz 分支(省 flate2/tar 两个依赖与一条测试面)。
    fn extract_archive(&self, bytes: &[u8], dest: &PathBuf, source_name: &str) -> Result<(), String> {
        if source_name.ends_with(".zip") {
            self.extract_zip(bytes, dest)
        } else {
            Err("不支持的插件包格式(仅支持 .zip)".to_string())
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

        // 解压炸弹防护(B-3):单文件/累计解压量/条目数均设上限。合法插件 zip <1MB,
        // 上限为其数倍余量;防 100MB 恶意包(压缩比极高)膨胀为任意大小填满磁盘。
        const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;   // 单条目 64MB
        const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;  // 累计 512MB
        const MAX_ENTRIES: usize = 10_000;               // 条目数上限
        let mut total_bytes: u64 = 0;

        for i in 0..archive.len() {
            if i >= MAX_ENTRIES {
                return Err(format!(
                    "插件包条目数超过上限({}): 疑似恶意包",
                    MAX_ENTRIES
                ));
            }
            let mut file = archive.by_index(i)
                .map_err(|e| e.to_string())?;

            // 归一化路径分隔符:zip 标准用正斜杠,但恶意/异常包可能混入反斜杠。
            // 统一转正斜杠后再做 Zip-Slip 检查,避免 Windows 上 PathBuf 把反斜杠
            // 当分隔符解析出歧义(与 protocol.rs 的路径归一化保持一致)。
            let raw_name = file.name().replace('\\', "/");
            let raw_name = raw_name.trim_end_matches('/');
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
                // 流式拷贝 + 解压量限制(防 zip bomb:解压体积不受压缩比控制,
                // 不能信任 zip 头声明的 sizes,按实际写入字节计数)。
                use std::io::{Read, Write};
                let mut entry_bytes: u64 = 0;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    entry_bytes += n as u64;
                    total_bytes += n as u64;
                    if entry_bytes > MAX_ENTRY_BYTES || total_bytes > MAX_TOTAL_BYTES {
                        return Err(format!(
                            "插件包解压超过大小上限(单文件 {}MB / 累计 {}MB): 疑似恶意包",
                            MAX_ENTRY_BYTES / (1024 * 1024),
                            MAX_TOTAL_BYTES / (1024 * 1024)
                        ));
                    }
                    outfile.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                }
            }
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
    /// 返回安装警告列表（依赖彻底不可用且无缓存回退时），随 InstallResult 透出。
    async fn install_dependencies(&self, plugin_id: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        let Ok(Some(manifest)) = self.get_manifest(plugin_id).await else {
            return warnings;
        };
        let Some(deps) = &manifest.dependencies else {
            return warnings;
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
                    warnings.push(format!(
                        "共享依赖 {} 下载失败，已回退使用缓存版本",
                        name
                    ));
                } else {
                    log::error!("Install dependency {} for {} failed: {}", name, plugin_id, e);
                    warnings.push(format!("共享依赖 {} 安装失败：{}", name, e));
                }
            }
        }
        warnings
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
