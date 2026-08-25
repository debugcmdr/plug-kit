use crate::registry::{Registry, DepEntry};
use sha2::Digest;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use fs2::FileExt;
use once_cell::sync::Lazy;

/// 共享 HTTP 客户端(依赖下载用，120s 超时——仅覆盖小请求/校验和等短操作)。
pub static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("reqwest client build failed")
});

/// 大文件下载专用客户端(R-4):**不设整体超时**(Client::timeout 覆盖整个请求含 body,
/// 慢网下载 200MB ffmpeg 会被 120s 上限误杀),仅连接超时;chunk 读取由
/// download_and_verify 内每块读超时兜底(挂起 60s 无数据即中止)。
pub static DOWNLOAD_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("download client build failed")
});

/// 依赖下载硬上限(单个文件,全局):1 GiB。
/// 远高于当前最大合法依赖(gyan.dev win full ffmpeg ~200MB)的 5 倍余量,
/// 覆盖未来工具增长;同时防止异常/恶意 URL 单次下载写爆磁盘。
/// 上限约束"单次下载"粒度,与插件/依赖数量无关(数量由引用计数+孤儿清理治理)。
const MAX_DEP_DOWNLOAD_BYTES: u64 = 1 << 30; // 1 GiB

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
        // 与 install_dependency/decrement_refcount 同用文件锁(R-20):无锁的
        // 读-改-写会与并发安装互相覆盖 registry,导致引用计数丢失更新。
        let _lock = self.acquire_lock()?;
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;
        
        let before_size = registry.total_size();
        let before_count = self.count_entries()?;
        
        // 收集 refcount=0 条目的磁盘目录(清理引用后需一并删除文件,否则
        // 只清 registry、磁盘空间不释放——"清理孤儿缓存"对用户是假清理)。
        let zero_paths = self.collect_zero_ref_paths(&registry);
        // Remove entries with refcount = 0
        self.remove_zero_ref_entries(&mut registry)?;
        
        registry.save(&self.registry_path)
            .map_err(|e| e.to_string())?;
        
        // 删除孤儿条目的缓存目录(refcount=0 = 无插件引用,删除安全,重装即重下)
        let mut removed_dirs = 0usize;
        for p in &zero_paths {
            if p.exists() {
                let _ = fs::remove_dir_all(p);
                removed_dirs += 1;
            }
        }
        if removed_dirs > 0 {
            log::info!("clean_orphans: removed {} cache dir(s)", removed_dirs);
        }
        
        let after_size = registry.total_size();
        let freed = before_size - after_size;
        
        Ok(serde_json::json!({
            "freed_bytes": freed,
            "freed_mb": freed as f64 / 1_000_000.0,
            "entries_removed": before_count - self.count_entries()?,
            "dirs_removed": removed_dirs,
        }))
    }

    /// 收集 registry 中 refcount=0 条目对应的磁盘目录路径(待清理)。
    fn collect_zero_ref_paths(&self, registry: &Registry) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut scan = |entries: &HashMap<String, HashMap<String, crate::registry::DepEntry>>| {
            for (_, platforms) in entries {
                for (platform, entry) in platforms {
                    if entry.refcount == 0 {
                        paths.push(self.cache_dir.join(&entry.path).join(platform));
                    }
                }
            }
        };
        scan(&registry.ffmpeg);
        scan(&registry.yt_dlp);
        scan(&registry.other);
        paths
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

        // 缓存命中:文件必须存在且有效(非空;sha256 固化时再校验内容一致)。
        // 防中断下载残留的 0 字节/半截文件被静默复用("幽灵缓存"——运行失败但
        // 报错与真实原因脱节,极难排查)。
        let bin_path = dest_dir.join(binary_name);
        if self.is_valid_cached_binary(&bin_path, sha256) {
            // Increment refcount
            self.increment_refcount(&mut registry, name, version, platform, ref_plugins)?;
            return Ok(dest_dir);
        }
        // 无效缓存(0 字节/损坏):删除后重新下载
        let _ = fs::remove_dir_all(&dest_dir);

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

    /// 校验缓存二进制有效:文件存在且非空;sha256 已固化时再校验内容一致。
    /// 未固化平台仅做非空检查(完整校验依赖 G-8 逐平台固化 sha256)。
    fn is_valid_cached_binary(&self, bin_path: &Path, expected_sha256: &str) -> bool {
        let Ok(meta) = std::fs::metadata(bin_path) else { return false };
        if !meta.is_file() || meta.len() == 0 {
            return false;
        }
        if expected_sha256.is_empty() {
            return true; // 未固化平台:仅非空
        }
        // 固化平台:读文件校验 sha256(依赖安装为一次性低频操作,成本可接受)
        match std::fs::read(bin_path) {
            Ok(bytes) => crate::security::validate_sha256(&bytes, expected_sha256).is_ok(),
            Err(_) => false,
        }
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
                // 引用按 plugin_id 去重后再递增,保证 refcount == refs.len()(卸载按 refs 扫描递减,两侧必须一致)。
                for r in ref_plugins {
                    if !entry.refs.iter().any(|x| x == r) {
                        entry.refs.push(r.to_string());
                        entry.refcount += 1;
                    }
                }
                entry.last_accessed = chrono::Utc::now().timestamp();
            }
        }
        Ok(())
    }

    /// 卸载插件时按 plugin_id 扫描 registry 递减其全部引用。
    /// 修复:旧实现按 (name, "latest") 精确寻址,而 registry 的 version key 是
    /// pinned 真实版本(如 2026.08.19),"latest" 永远查不到 → 递减恒为 no-op,
    /// 卸载后依赖成为永久孤儿。现在引用关系完全由 refs 列表表达,与安装时
    /// 的递增(去重 push refs)严格对称。refcount 归 0 的条目由 clean_orphans 清理。
    pub async fn decrement_refcount_for_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let _lock = self.acquire_lock()?;
        let mut registry = Registry::load_or_new(&self.registry_path)
            .map_err(|e| e.to_string())?;

        let mut changed = false;
        for bucket in [&mut registry.ffmpeg, &mut registry.yt_dlp, &mut registry.other] {
            for (_, platforms) in bucket.iter_mut() {
                for (_, entry) in platforms.iter_mut() {
                    let before = entry.refs.len();
                    entry.refs.retain(|r| r != plugin_id);
                    let removed = before - entry.refs.len();
                    if removed > 0 {
                        entry.refcount = entry.refcount.saturating_sub(removed as u32);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            registry.save(&self.registry_path).map_err(|e| e.to_string())?;
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
    /// 流式下载:逐 chunk 写临时文件 + 增量 sha256(内存恒定,不随文件大小增长);
    /// 超过 MAX_DEP_DOWNLOAD_BYTES 硬上限即 abort 下载 + 删除临时文件 + 报错。
    async fn download_and_verify(
        &self,
        src: &str,
        dest: &Path,
        sha256: &str,
        checksum_url: Option<&str>,
        original_url: &str,
        binary_name: &str,
    ) -> Result<(), String> {
        // 大文件用 DOWNLOAD_CLIENT(无整体超时,见 R-4);连接 20s 超时由 client 兜底。
        let resp = DOWNLOAD_CLIENT.get(src)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("Download failed with status {}", resp.status()));
        }

        // 校验目标先就绪:1) 固定 sha256(若配置);2) 否则从官方校验和文件动态校验(yt-dlp)。
        // 期望值为空时(未固化平台且无 checksum 源)仅做大小上限,不做哈希校验(见 G-8)。
        let expected = if !sha256.is_empty() {
            sha256.to_string()
        } else if let Some(cu) = checksum_url {
            let asset_name = original_url.rsplit('/').next().unwrap_or(binary_name);
            Self::fetch_checksum_for(cu, asset_name).await?
        } else {
            String::new()
        };

        // 流式下载:逐 chunk 写 tmp + 增量哈希(内存恒定,不收集全部块)。
        // 用 DOWNLOAD_CLIENT(无整体超时)发请求;每个 chunk 加 60s 读超时,
        // 防服务器挂起不吐数据(R-4:整体 120s 超时会让慢网大文件下载必失败)。
        let tmp = dest.join("download.tmp");
        let mut out = fs::File::create(&tmp)
            .map_err(|e| format!("Create tmp file: {}", e))?;
        let mut resp = resp;
        let mut hasher = sha2::Sha256::new();
        let mut total: u64 = 0;
        loop {
            let chunk = match tokio::time::timeout(
                std::time::Duration::from_secs(60),
                resp.chunk(),
            )
            .await
            {
                Ok(Ok(Some(c))) => c,
                Ok(Ok(None)) => break, // 正常读完
                Ok(Err(e)) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(format!("Read chunk failed: {}", e));
                }
                Err(_) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(format!(
                        "下载读取超时(60s 无数据),已中止: {}",
                        src
                    ));
                }
            };
            total += chunk.len() as u64;
            // 硬上限:超过阈值立即中止(chunk 循环退出即放弃后续读取 = abort http),
            // 删除临时文件并返回错误。
            if total > MAX_DEP_DOWNLOAD_BYTES {
                drop(resp);
                drop(out);
                let _ = fs::remove_file(&tmp);
                return Err(format!(
                    "下载超过大小上限({} MB),已中止: {}",
                    MAX_DEP_DOWNLOAD_BYTES / (1024 * 1024),
                    src
                ));
            }
            hasher.update(&chunk);
            out.write_all(&chunk)
                .map_err(|e| format!("Write chunk failed: {}", e))?;
        }
        out.flush().map_err(|e| format!("Flush tmp file: {}", e))?;
        drop(out);

        // 增量哈希结果与期望值比较(期望值非空时);失败即删 tmp,交由上层降级其他源。
        let actual = format!("{:x}", hasher.finalize());
        if !expected.is_empty() && actual != expected {
            let _ = fs::remove_file(&tmp);
            return Err(format!(
                "SHA256 mismatch: expected {}, got {}",
                expected, actual
            ));
        }

        // 落盘:rename 到最终名(tmp 与最终名同目录 = 同文件系统,必成功)。
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
                // 过滤隐藏/下载中残留 + 0 字节损坏文件(中断残留不可回退复用)
                let ok = p.metadata().map(|m| m.is_file() && m.len() > 0).unwrap_or(false);
                if fname.starts_with('.') || fname.starts_with("download.tmp") || !ok {
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
                // 过滤隐藏/下载中残留 + 0 字节损坏文件(中断残留不可回退复用)
                let ok = p.metadata().map(|m| m.is_file() && m.len() > 0).unwrap_or(false);
                if fname.starts_with('.') || fname.starts_with("download.tmp") || !ok {
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
