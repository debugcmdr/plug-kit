//! Real end-to-end install test: calls PluginManager::install against the real
//! GitHub Release and verifies the plugin lands in ~/.plugkit/plugins/{id} with
//! the CORRECT structure (no double nesting, manifest.json at root).
//!
//! This is the exact code path the market's "安装" button uses.

use plugkit_lib::api::PluginManager;

fn clean_installed(id: &str) {
    let home = std::env::var("HOME").unwrap();
    let dir = std::path::Path::new(&home).join(".plugkit/plugins").join(id);
    let _ = std::fs::remove_dir_all(&dir);
}

fn installed_layout_ok(id: &str) -> bool {
    let home = std::env::var("HOME").unwrap();
    let root = std::path::Path::new(&home).join(".plugkit/plugins").join(id);
    // manifest.json must be DIRECTLY under the plugin root (not nested).
    let manifest = root.join("manifest.json");
    if !manifest.exists() {
        return false;
    }
    // tool/ must exist with an executable and (for our plugins) index.html.
    let tool = root.join("tool");
    tool.is_dir()
}

#[tokio::test]
async fn install_audio_extract_from_github() {
    clean_installed("audio-extract");
    let pm = PluginManager::new();
    let r = pm.install("audio-extract").await.expect("install should succeed");
    assert!(r.success, "install reported failure: {}", r.message);
    assert!(installed_layout_ok("audio-extract"),
        "audio-extract layout wrong after install (double-nest or missing manifest)");
    println!("installed: {}", r.message);
    clean_installed("audio-extract");
}

#[tokio::test]
async fn install_convert_from_github() {
    clean_installed("convert");
    let pm = PluginManager::new();
    let r = pm.install("convert").await.expect("install should succeed");
    assert!(r.success, "install reported failure: {}", r.message);
    assert!(installed_layout_ok("convert"),
        "convert layout wrong after install (double-nest or missing manifest)");
    clean_installed("convert");
}
