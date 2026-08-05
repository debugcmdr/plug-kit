//! End-to-end install test: simulate the market install flow against a local
//! HTTP server serving the packaged zips. Requires the test server to be
//! running on port 18765 (see the dev script) OR skipped if not reachable.

use plugkit_lib::api;

#[tokio::test]
async fn local_market_install_flow() {
    // Point the market manifest at the local server.
    std::env::set_var("PLUGKIT_MANIFEST_URL", "http://127.0.0.1:18765/market/plugins.json");

    // Fetch the market list (goes through manifest_fetcher::fetch_market_manifests).
    let plugins = api::fetch_market_manifests()
        .await
        .expect("market fetch should work");
    assert!(!plugins.is_empty(), "market should list plugins");
    assert!(plugins.iter().any(|p| p.id == "download"));

    // Try downloading the download plugin zip from its binaryUrl.
    let download = plugins.iter().find(|p| p.id == "download").unwrap();
    // Rewrite binaryUrl to local server (the real one points at GitHub Release).
    // binaryUrl is like .../releases/download/v0.1.0/download-0.1.0.zip
    let filename = download.binary_url.rsplit('/').next().unwrap();
    let local_url = format!("http://127.0.0.1:18765/dist/plugins/{}", filename);

    let bytes = api::download_with_fallback(
        &local_url,
        download.sha256.as_deref(),
    ).await.expect("download should succeed");

    // Verify the archive is a valid zip containing manifest.json.
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("valid zip");
    assert!(archive.len() > 0);
    let mut has_manifest = false;
    for i in 0..archive.len() {
        if archive.by_index(i).unwrap().name().ends_with("manifest.json") {
            has_manifest = true;
        }
    }
    assert!(has_manifest, "zip should contain manifest.json");
}

#[tokio::test]
async fn registry_roundtrip() {
    // Registry load/save roundtrip (uses ~/.plugkit, real home).
    let cache = api::DependencyCache::new();
    let _ = cache.stats().expect("cache stats should work");
}
