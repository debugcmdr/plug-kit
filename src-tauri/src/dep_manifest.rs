use serde::{Deserialize, Serialize};

/// 内置共享依赖清单:定义 ffmpeg / yt-dlp 的获取方式。
/// 插件 manifest 的 `dependencies` 声明依赖名 + semver 约束,运行时由
/// DependencyCache 从这里解析下载源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepSpec {
    pub name: String,
    /// 各平台下载 URL(原始地址,可被镜像降级)
    pub url: String,
    pub sha256: String,
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
        url: format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", asset),
        sha256: String::new(), // 官方发布不固定 sha256
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
    // 用 `which` 探测系统 PATH
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
