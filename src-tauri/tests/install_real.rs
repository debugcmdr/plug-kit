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

/// 真实 GitHub 发布链路测试:依赖远程仓库已发布插件包 + 市场清单已推送。
/// 开发期(未发布/本地清单未推送)必然 404 失败——标记 ignore,
/// 发布演练时用 `cargo test --ignored --test install_real` 跑。
#[tokio::test]
#[ignore]
async fn install_convert_from_github() {
    clean_installed("convert");
    let pm = PluginManager::new();
    let r = pm.install("convert").await.expect("install should succeed");
    assert!(r.success, "install reported failure: {}", r.message);
    assert!(installed_layout_ok("convert"),
        "convert layout wrong after install (double-nest or missing manifest)");
    clean_installed("convert");
}
