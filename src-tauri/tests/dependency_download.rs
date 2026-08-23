//! ffmpeg 共享依赖真实下载链路测试(手动运行,真实网络):
//!   cargo test --test dependency_download -- --ignored
//!
//! 验证:下载(含镜像降级)→ sha256 校验 → 缓存落盘 → refcount 登记 →
//! 二次调用只递增引用不重复下载 → 清理回收。
//! 注意:本机 PATH 有 ffmpeg 时 ensure_dependency 会复用系统,故直接调
//! install_dependency 强制走下载链路。

use plugkit_lib::api::DependencyCache;

const FFMPEG_VERSION: &str = "b6.1.1";
const DARWIN_ARM64_URL: &str = "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-darwin-arm64";
const DARWIN_ARM64_SHA256: &str = "a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584";

fn cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join(".plugkit/cache/deps/ffmpeg").join(FFMPEG_VERSION).join("macos-arm64")
}

/// 用系统 shasum 计算文件 sha256(shasum 在 macOS/Linux 均可用)。
fn sha256_of(path: &std::path::Path) -> String {
    let out = std::process::Command::new("shasum")
        .args(["-a", "256", path.to_str().unwrap()])
        .output()
        .expect("shasum should run");
    assert!(out.status.success(), "shasum failed");
    String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap().to_string()
}

#[tokio::test]
#[ignore = "真实网络下载(45MB),手动运行"]
async fn ffmpeg_download_verify_and_refcount() {
    let dc = DependencyCache::new();

    // 干净起点:清掉历史残留(异常中断可能留下无 registry 登记的磁盘目录)
    let _ = std::fs::remove_dir_all(cache_dir());

    // 1. 首次下载:下载 + sha256 校验 + 落盘 + refcount=1
    let dir = dc.install_dependency(
        "ffmpeg", FFMPEG_VERSION, "macos-arm64",
        DARWIN_ARM64_URL, DARWIN_ARM64_SHA256, None, "ffmpeg", &["test-plugin"],
    ).await.expect("ffmpeg download+verify should succeed");

    let exe = dir.join("ffmpeg");
    assert!(exe.exists(), "ffmpeg binary should exist at {}", exe.display());
    let actual = sha256_of(&exe);
    assert_eq!(actual, DARWIN_ARM64_SHA256, "downloaded binary sha256 mismatch");

    // 2. 二次调用:缓存命中,只递增引用,不重复下载
    let dir2 = dc.install_dependency(
        "ffmpeg", FFMPEG_VERSION, "macos-arm64",
        DARWIN_ARM64_URL, DARWIN_ARM64_SHA256, None, "ffmpeg", &["test-plugin", "test-plugin-2"],
    ).await.expect("second call should hit cache");
    assert_eq!(dir, dir2, "second install should reuse same cache dir");

    // 3. 卸载场景:递减引用并清理孤儿
    dc.decrement_refcount_for_plugin("test-plugin").await.unwrap();
    dc.decrement_refcount_for_plugin("test-plugin-2").await.unwrap();
    dc.clean_orphans().await.unwrap();
    assert!(!cache_dir().exists(), "orphaned ffmpeg cache should be cleaned up");
}
