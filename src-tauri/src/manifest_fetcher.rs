use serde::{Deserialize, Serialize};

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

/// Fetch the remote plugin marketplace manifest.
/// Falls back to the bundled snapshot on failure (offline / GitHub blocked).
pub async fn fetch_market_manifests() -> Result<Vec<ManifestSummary>, String> {
    // TODO: read remote plugins.json URL from settings (github raw), with timeout.
    // For now, serve the bundled fallback so the market UI works end-to-end.
    Ok(bundled_manifests())
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
    for mirror in settings.download_mirrors.iter().filter(|m| m.enabled) {
        if !mirror.prefix.is_empty() {
            sources.push(format!("{}{}", mirror.prefix, url));
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
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Build client: {}", e))?;

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
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
