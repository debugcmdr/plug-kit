use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;

/// 共享 HTTP 客户端(市场清单拉取用，15s 超时)。
static MARKET_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("reqwest client build failed")
});

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestList {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub plugins: Vec<ManifestSummary>,
}

/// Default remote marketplace manifest URL (served via GitHub Raw).
/// Points at the `market/plugins.json` file in the main repo.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/debugcmdr/plug-kit/main/market/plugins.json";

/// Fetch the remote plugin marketplace manifest.
/// Falls back to the bundled snapshot on failure (offline / GitHub blocked).
pub async fn fetch_market_manifests() -> Result<Vec<ManifestSummary>, String> {
    match fetch_remote_manifests().await {
        Ok(plugins) if !plugins.is_empty() => Ok(plugins),
        Ok(_) => {
            log::warn!("Remote manifest empty, falling back to bundled snapshot");
            Ok(bundled_manifests())
        }
        Err(e) => {
            log::warn!("Remote manifest fetch failed ({}), using bundled snapshot", e);
            Ok(bundled_manifests())
        }
    }
}

/// Fetch the remote marketplace manifest from GitHub Raw with a timeout.
async fn fetch_remote_manifests() -> Result<Vec<ManifestSummary>, String> {
    let url = std::env::var("PLUGKIT_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_string());
    let resp = MARKET_CLIENT.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let list: ManifestList = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Parse manifest: {}", e))?;
    Ok(list.plugins)
}

/// Bundled fallback snapshot — compiled in, used when offline or first run.
pub fn bundled_manifests() -> Vec<ManifestSummary> {
    let bundled = include_str!("../assets/fallback-manifests.json");
    let list: ManifestList = serde_json::from_str(bundled)
        .unwrap_or(ManifestList { schema_version: 1, plugins: vec![] });
    list.plugins
}

/// Download a plugin archive via configured mirrors, verifying SHA256.
/// Original URL is tried first, then each enabled mirror.
pub async fn download_with_fallback(
    url: &str,
    expected_sha256: Option<&str>,
) -> Result<Vec<u8>, String> {
    let settings_value = crate::config_mgr::ConfigMgr::new()
        .get_settings()
        .map_err(|e| e.to_string())?;
    let settings: crate::config_mgr::Settings =
        serde_json::from_value(settings_value)
            .map_err(|e| format!("Parse settings: {}", e))?;

    // Original URL first, then enabled mirrors.
    let mut sources: Vec<String> = vec![url.to_string()];
    // Mirrors only apply to GitHub-hosted downloads (GitHub is the only
    // source that's blocked in some regions). Local / other URLs go direct.
    if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
        for mirror in settings.download_mirrors.iter().filter(|m| m.enabled) {
            if !mirror.prefix.is_empty() {
                sources.push(format!("{}{}", mirror.prefix, url));
            }
        }
    }

    let mut last_err = String::from("No download source attempted");
    for source in sources {
        match download_single(&source, expected_sha256).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                log::warn!("Mirror {} failed: {}", source, e);
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
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

    if let Some(sha) = expected_sha256 {
        crate::security::validate_sha256(&bytes, sha)
            .map_err(|e| e.to_string())?;
    }
    Ok(bytes.to_vec())
}
