use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// 共享 HTTP 客户端(市场清单拉取用，8s 超时)。
static MARKET_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .expect("reqwest client build failed")
});

/// 插件包下载硬上限:100 MB。合法插件 zip < 1 MB,此上限为其 100 倍余量,
/// 防市场清单异常/被篡改时声明超大 zip 占用内存/磁盘(与依赖下载上限分开:
/// 依赖可达 1 GiB,插件包极小故上限更紧)。
const PLUGIN_ZIP_MAX_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

/// Market plugin entry — light metadata used by the store UI.
/// This is what `plugins.json` (GitHub Raw) contains; the full
/// runtime manifest lives inside each installed plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(rename = "binaryUrl")]
    pub binary_url: String,
    #[serde(rename = "releaseUrl", default)]
    pub release_url: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// 插件包 Ed25519 签名(base64)。可选:配置了 OFFICIAL_PUBLIC_KEY 后强制校验。
    #[serde(default)]
    pub signature: Option<String>,
    /// 签名者标识(如 GitHub 用户名),仅展示用途。
    #[serde(default)]
    pub signer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestList {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub plugins: Vec<ManifestSummary>,
}

/// Default remote marketplace manifest URL (served via GitHub Raw).
/// Points at the `market/plugins.json` file in the main repo.
/// 切换仓库时:改这里,或用环境变量 PLUGKIT_MANIFEST_URL 覆盖。
/// (注意与 scripts/*.sh 的 PLUGKIT_REPO 保持一致)
pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/debugcmdr/plug-kit/main/market/plugins.json";

/// 市场清单内存缓存:TTL 5 分钟,`market_refresh` 清空后强制拉取远程。
/// 避免市场面板 5 分钟轮询 + 每次进入都发一次 HTTP。
static MANIFEST_CACHE: Lazy<Mutex<Option<(std::time::Instant, Vec<ManifestSummary>)>>> =
    Lazy::new(|| Mutex::new(None));
const MANIFEST_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Fetch the remote plugin marketplace manifest.
/// Falls back to the bundled snapshot on failure (offline / GitHub blocked).
/// Results are cached in-memory for 5 minutes.
pub async fn fetch_market_manifests() -> Result<Vec<ManifestSummary>, String> {
    // 缓存命中(TTL 内且有数据)直接返回
    {
        let cache = MANIFEST_CACHE.lock().unwrap();
        if let Some((at, plugins)) = cache.as_ref() {
            if at.elapsed() < MANIFEST_CACHE_TTL && !plugins.is_empty() {
                return Ok(plugins.clone());
            }
        }
    }

    let result = match fetch_remote_manifests().await {
        Ok(plugins) if !plugins.is_empty() => {
            log::info!("Market manifest refreshed ({} plugins)", plugins.len());
            Ok(plugins)
        }
        Ok(_) => {
            log::warn!("Remote manifest empty, falling back to bundled snapshot");
            Ok(bundled_manifests())
        }
        Err(e) => {
            log::warn!("Remote manifest fetch failed ({}), using bundled snapshot", e);
            Ok(bundled_manifests())
        }
    };

    if let Ok(plugins) = &result {
        *MANIFEST_CACHE.lock().unwrap() = Some((std::time::Instant::now(), plugins.clone()));
    }
    result
}

/// 清空市场清单缓存(强制下次 fetch 拉取远程)。
pub fn invalidate_market_cache() {
    *MANIFEST_CACHE.lock().unwrap() = None;
}

/// Fetch the remote marketplace manifest from GitHub Raw.
/// Tries the direct URL plus every configured mirror **in parallel** and
/// returns the first successful (non-empty) response — so a slow/unreachable
/// GitHub direct connection no longer blocks the market refresh (the old code
/// did a serial 15s direct GET). Mirrors only apply to GitHub-hosted URLs.
async fn fetch_remote_manifests() -> Result<Vec<ManifestSummary>, String> {
    let base_url = std::env::var("PLUGKIT_MANIFEST_URL")
        .unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_string());

    // 候选源:原始 URL + 镜像前缀(仅 GitHub 托管适用镜像前缀)。
    let mut sources: Vec<String> = vec![base_url.clone()];
    if base_url.contains("githubusercontent.com") || base_url.contains("github.com") {
        for mirror in enabled_mirrors() {
            let m = mirror.trim_end_matches('/');
            sources.push(format!("{}/{}", m, base_url));
        }
    }
    sources.sort();
    sources.dedup();

    // 并行拉取所有候选源,谁先成功(non-empty)用谁。
    let client = MARKET_CLIENT.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<ManifestSummary>>(sources.len().max(1));
    let mut handles = Vec::with_capacity(sources.len());
    for src in sources {
        let client = client.clone();
        let tx = tx.clone();
        handles.push(tokio::spawn(async move {
            match client.get(&src).send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        return;
                    }
                    let bytes = match resp.bytes().await {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    let list: ManifestList = match serde_json::from_slice(&bytes) {
                        Ok(l) => l,
                        Err(_) => return,
                    };
                    if !list.plugins.is_empty() {
                        let _ = tx.send(list.plugins).await;
                    }
                }
                Err(_) => return,
            }
        }));
    }

    match rx.recv().await {
        Some(plugins) => {
            for h in handles {
                h.abort();
            }
            Ok(plugins)
        }
        None => {
            for h in handles {
                let _ = h.await;
            }
            Err("All manifest sources failed".into())
        }
    }
}

/// Bundled fallback snapshot — compiled in, used when offline or first run.
pub fn bundled_manifests() -> Vec<ManifestSummary> {
    let bundled = include_str!("../assets/fallback-manifests.json");
    let list: ManifestList = serde_json::from_str(bundled)
        .unwrap_or(ManifestList { schema_version: 1, plugins: vec![] });
    list.plugins
}

/// 内置镜像候选池(前缀需带尾斜杠,直接拼接在 GitHub URL 前)。
/// 公共代理镜像可能随时失效——download_with_fallback 会按序尝试并自动跳过
/// 不可用的;确认失败的镜像在本会话内进入黑名单,后续下载不再重复尝试。
/// 高级用户可用环境变量 PLUGKIT_MIRRORS 覆盖(逗号分隔前缀列表)。
/// 注:2026-08 实测 ghproxy.com / mirror.ghproxy.com 已失效,勿再添加回列表。
const MIRROR_CANDIDATES: &[&str] = &[
    "https://gh-proxy.com/",
    "https://ghfast.top/",
];

/// 本会话内已确认不可用的镜像(避免每次下载都先试死镜像浪费时间)。
static DEAD_MIRRORS: Lazy<Mutex<std::collections::HashSet<String>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

/// 供依赖下载(dependency_cache)共用:查询镜像是否已标记为不可用。
pub(crate) fn is_dead_mirror(source: &str) -> bool {
    DEAD_MIRRORS.lock().unwrap().contains(source)
}

/// 供依赖下载(dependency_cache)共用:标记镜像本会话内不可用。
pub(crate) fn mark_dead_mirror(source: &str) {
    DEAD_MIRRORS.lock().unwrap().insert(source.to_string());
}

/// 解析生效的镜像前缀列表:优先环境变量 PLUGKIT_MIRRORS,否则内置候选池。
pub(crate) fn enabled_mirrors() -> Vec<String> {
    if let Ok(v) = std::env::var("PLUGKIT_MIRRORS") {
        let list: Vec<String> = v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !list.is_empty() {
            return list;
        }
    }
    MIRROR_CANDIDATES.iter().map(|s| s.to_string()).collect()
}

/// Download a plugin archive via configured mirrors, verifying SHA256.
/// Original URL is tried first, then each enabled mirror (dead ones skipped).
pub async fn download_with_fallback(
    url: &str,
    expected_sha256: Option<&str>,
) -> Result<Vec<u8>, String> {
    // Original URL first, then enabled mirrors.
    let mut sources: Vec<String> = vec![url.to_string()];
    // Mirrors only apply to GitHub-hosted downloads (GitHub is the only
    // source that's blocked in some regions). Local / other URLs go direct.
    if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
        for mirror in enabled_mirrors() {
            sources.push(format!("{}{}", mirror, url));
        }
    }

    let mut last_err = String::from("No download source attempted");
    for source in sources {
        // 跳过本会话内已确认不可用的镜像(原始 URL 不参与黑名单)
        if source != url && DEAD_MIRRORS.lock().unwrap().contains(&source) {
            continue;
        }
        match download_single(&source, expected_sha256).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                log::warn!("Mirror {} failed: {}", source, e);
                // 镜像失败 → 记入黑名单(原始 URL 失败可能是临时网络波动,不标记)
                if source != url {
                    DEAD_MIRRORS.lock().unwrap().insert(source.clone());
                }
                last_err = format!("{}: {}", source, e);
            }
        }
    }
    Err(format!("All download sources failed: {}", last_err))
}

async fn download_single(url: &str, expected_sha256: Option<&str>) -> Result<Vec<u8>, String> {
    let resp = crate::dependency_cache::HTTP_CLIENT.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    // 流式累计 + 大小上限(读取中即检查,超限 abort;插件包体积小,全量内存可接受)
    let mut resp = resp;
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > PLUGIN_ZIP_MAX_BYTES {
            return Err(format!(
                "插件包超过大小上限({} MB): {}",
                PLUGIN_ZIP_MAX_BYTES / (1024 * 1024),
                url
            ));
        }
    }

    if let Some(sha) = expected_sha256 {
        crate::security::validate_sha256(&bytes, sha)
            .map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}
