//! 预装工具机制:把随应用打包的插件(Resources/plugins/)复制到
//! ~/.plugkit/plugins/,使默认工具无需去市场下载即可使用。
//!
//! 复制仅发生在插件尚未安装时(幂等)。用户卸载后不会自动恢复。

use std::path::{Path, PathBuf};
use tauri::Manager;

/// 预装插件 id 列表(与 Resources/plugins/ 下的目录一致)。
const PREINSTALLED: &[&str] = &["download", "convert", "audio-extract"];

/// 应用资源目录下的插件根(打包后位于 Resources/plugins/)。
fn bundled_plugins_root(app: &tauri::AppHandle) -> PathBuf {
    app.path().resource_dir().unwrap_or_default().join("plugins")
}

/// 用户插件目录。
fn user_plugins_root() -> PathBuf {
    dirs::home_dir()
        .map(|d| d.join(".plugkit/plugins"))
        .unwrap_or_default()
}

/// 启动时执行预装检查:对每个预装插件,若用户目录缺失则从资源复制。
pub fn ensure_preinstalled(app: &tauri::AppHandle) {
    let bundled = bundled_plugins_root(app);
    let user = user_plugins_root();

    for id in PREINSTALLED {
        let src = bundled.join(id);
        let dst = user.join(id);
        // 已安装则跳过(含用户主动卸载 → 不恢复)
        if dst.exists() {
            continue;
        }
        if !src.exists() || !src.join("manifest.json").exists() {
            log::warn!("Bundled plugin '{}' missing or invalid, skip preinstall", id);
            continue;
        }
        match copy_dir(&src, &dst) {
            Ok(_) => log::info!("Preinstalled plugin '{}' -> {:?}", id, dst),
            Err(e) => log::warn!("Preinstall '{}' failed: {}", id, e),
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            // 保留可执行权限
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&from) {
                    let _ = std::fs::set_permissions(&to, meta.permissions());
                }
            }
        }
    }
    Ok(())
}
