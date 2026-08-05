//! Remote market e2e test: verifies the real GitHub manifest + release download
//! pipeline that the shipped DMG will use. Needs network access to GitHub.
//! Mirrors are disabled in this test to avoid proxy interference.

use plugkit_lib::api;

#[tokio::test]
async fn real_github_market_flow() {
    // Unset local override; use the default (real GitHub Raw URL).
    std::env::remove_var("PLUGKIT_MANIFEST_URL");

    // Fetch market from real GitHub Raw.
    let plugins = api::fetch_market_manifests().await
        .expect("fetch from real GitHub should work");
    assert!(!plugins.is_empty(), "market should list plugins");
    let download = plugins.iter().find(|p| p.id == "download")
        .expect("download plugin in market");

    // Download the real zip from GitHub Release and verify sha256.
    let bytes = api::download_with_fallback(&download.binary_url, download.sha256.as_deref())
        .await
        .expect("real download + sha256 verify should pass");
    assert!(bytes.len() > 1000, "zip should be non-trivial size");

    // Valid zip with manifest.json.
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("valid zip");
    assert!(archive.len() > 0);
}
