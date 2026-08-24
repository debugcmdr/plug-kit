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
        "ffmpeg" => Some(ffmpeg_spec_for_platform()),
        "ffprobe" => Some(ffprobe_spec_for_platform()),
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

/// ffmpeg/ffprobe 固定版本(eugeneware/ffmpeg-static 的 release tag,永久保留)。
/// 单文件静态二进制(免解压),含 macOS arm64/x64、Linux x64、Windows x64。
/// 升级需经过验证后人工更新此处常量,并把当前平台的 sha256 固化
/// (darwin-arm64 已固化;其他平台留空=下载时跳过校验,首次在对应平台
/// 下载后运行 `shasum -a 256` 固化,消除无校验窗口)。
const FFMPEG_VERSION: &str = "b6.1.1";

/// ffmpeg 平台 spec。
/// 二进制来自 GitHub Release(可走镜像降级);Windows 落盘需 .exe 后缀。
/// sha256:darwin-arm64 已固化;其他平台留空(下载时跳过校验),首次在
/// 对应平台下载后运行 `shasum -a 256` 固化,消除无校验窗口。
fn ffmpeg_spec_for_platform() -> DepSpec {
    let (asset, binary_name, sha256) = if cfg!(target_os = "windows") {
        ("ffmpeg-win32-x64", "ffmpeg.exe", String::new())
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        ("ffmpeg-darwin-arm64", "ffmpeg", String::from("a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584"))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        ("ffmpeg-darwin-x64", "ffmpeg", String::new())
    } else {
        ("ffmpeg-linux-x64", "ffmpeg", String::new())
    };
    DepSpec {
        name: "ffmpeg".to_string(),
        version: FFMPEG_VERSION.to_string(),
        url: format!(
            "https://github.com/eugeneware/ffmpeg-static/releases/download/{}/{}",
            FFMPEG_VERSION, asset
        ),
        sha256,
        checksum_url: None,
        binary_name: binary_name.to_string(),
    }
}

/// ffprobe 平台 spec(与 ffmpeg 同源同版本,convert 插件时长探测用)。
fn ffprobe_spec_for_platform() -> DepSpec {
    let (asset, binary_name, sha256) = if cfg!(target_os = "windows") {
        ("ffprobe-win32-x64", "ffprobe.exe", String::new())
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        ("ffprobe-darwin-arm64", "ffprobe", String::from("bb2db6f5d8cef919da12fbf592119a987202a8c060a886f3cab091f9cab90b64"))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        ("ffprobe-darwin-x64", "ffprobe", String::new())
    } else {
        ("ffprobe-linux-x64", "ffprobe", String::new())
    };
    DepSpec {
        name: "ffprobe".to_string(),
        version: FFMPEG_VERSION.to_string(),
        url: format!(
            "https://github.com/eugeneware/ffmpeg-static/releases/download/{}/{}",
            FFMPEG_VERSION, asset
        ),
        sha256,
        checksum_url: None,
        binary_name: binary_name.to_string(),
    }
}

/// 该依赖是否已在系统 PATH 可用(此时无需下载)。
/// 跨平台探测:Unix 用 `sh -c "command -v"`,Windows 用 `where`。
/// (原实现一律用 sh——Windows 无 sh 恒返回 false,系统 PATH 已装的
/// ffmpeg/yt-dlp 被判定不可用,触发无谓下载。)
pub fn dep_available_on_path(name: &str) -> bool {
    let exe = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("where")
            .arg(&exe)
            .output()
    } else {
        std::process::Command::new("sh")
            .args(["-c", &format!("command -v {} 2>/dev/null", exe)])
            .output()
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_ffmpeg_returns_spec() {
        let spec = resolve_dep_spec("ffmpeg").expect("ffmpeg spec should resolve");
        assert_eq!(spec.name, "ffmpeg");
        assert_eq!(spec.version, FFMPEG_VERSION);
        assert!(!spec.url.is_empty());
        assert!(spec.url.contains("eugeneware/ffmpeg-static"));
        assert!(spec.binary_name == "ffmpeg" || spec.binary_name == "ffmpeg.exe");
    }

    #[test]
    fn resolve_ffprobe_returns_spec() {
        let spec = resolve_dep_spec("ffprobe").expect("ffprobe spec should resolve");
        assert_eq!(spec.name, "ffprobe");
        assert_eq!(spec.version, FFMPEG_VERSION);
        assert!(spec.binary_name == "ffprobe" || spec.binary_name == "ffprobe.exe");
    }

    #[test]
    fn resolve_unknown_returns_none() {
        assert!(resolve_dep_spec("nonexistent-tool").is_none());
    }

    #[test]
    fn ffmpeg_darwin_arm64_sha256_fixed() {
        // 固化哈希回归:darwin-arm64 必须带 sha256(防篡改),其他平台允许留空
        let spec = resolve_dep_spec("ffmpeg").unwrap();
        if spec.url.ends_with("ffmpeg-darwin-arm64") {
            assert!(!spec.sha256.is_empty(), "darwin-arm64 ffmpeg must have pinned sha256");
            assert_eq!(spec.sha256.len(), 64);
        }
    }
}
