use serde::{Deserialize, Serialize};

/// 内置共享依赖清单:定义 ffmpeg / yt-dlp 的获取方式。
/// 插件 manifest 的 `dependencies` 声明依赖名 + semver 约束,运行时由
/// DependencyCache 从这里解析下载源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepSpec {
    pub name: String,
    /// 固定版本号(pinned,可复现)。升级是经过验证后的有意识动作,不是 latest 漂移。
    pub version: String,
    /// 各平台下载 URL(原始地址,可被镜像降级)
    pub url: String,
    /// 固定 sha256(优先);为空时若 checksum_url 提供,则从官方校验和文件动态校验。
    pub sha256: String,
    /// 官方校验和文件 URL(如 yt-dlp 的 SHA2-256SUMS),用于 sha256 缺失时的校验。
    pub checksum_url: Option<String>,
    /// 安装后缓存里的可执行文件名
    pub binary_name: String,
}

/// 解析当前平台 + 指定依赖名,返回可安装的 DepSpec。
/// 若依赖在系统 PATH 上已可用(如 ffmpeg),返回 None → 优先复用系统。
pub fn resolve_dep_spec(name: &str) -> Option<DepSpec> {
    match name.to_lowercase().as_str() {
        "yt-dlp" => Some(ytdlp_spec_for_platform()),
        _ => None,
    }
}

/// yt-dlp 当前固定的稳定版本(升级需经过验证后人工更新此处常量)。
/// 固定版本 + 官方 SHA2-256SUMS 校验,替代原先 latest + 无校验的供应链漂移。
const YTDLP_VERSION: &str = "2026.08.19";

/// yt-dlp 按平台选择独立二进制(内置 Python,无 shebang 依赖)。
/// macOS: yt-dlp_macos / Linux: yt-dlp_linux / Windows: yt-dlp.exe
fn ytdlp_spec_for_platform() -> DepSpec {
    let (asset, binary_name) = if cfg!(target_os = "windows") {
        ("yt-dlp.exe", "yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        ("yt-dlp_macos", "yt-dlp")
    } else {
        ("yt-dlp_linux", "yt-dlp")
    };
    DepSpec {
        name: "yt-dlp".to_string(),
        version: YTDLP_VERSION.to_string(),
        url: format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/{}",
            YTDLP_VERSION, asset
        ),
        // 官方为固定版本发布 SHA2-256SUMS,安装时动态下载校验该资产的 sha256。
        sha256: String::new(),
        checksum_url: Some(format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/SHA2-256SUMS",
            YTDLP_VERSION
        )),
        binary_name: binary_name.to_string(),
    }
}

/// 该依赖是否已在系统 PATH 可用(此时无需下载)。
pub fn dep_available_on_path(name: &str) -> bool {
    let exe = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    // 用 system `which` 探测 PATH，与 dep_manifest 工具目录无关
    let output = std::process::Command::new("sh")
        .args(["-c", &format!("command -v {} 2>/dev/null", exe)])
        .output();
    matches!(output, Ok(o) if o.status.success() && !o.stdout.is_empty())
}

/// 当前平台字符串(windows-x64 / macos-arm64 / ...)。
pub fn current_platform() -> String {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64".to_string()
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64".to_string()
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64".to_string()
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64".to_string()
    } else {
        "unknown".to_string()
    }
}
