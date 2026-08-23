//! 预装工具机制:把随应用打包的插件(Resources/plugins/)复制到
//! ~/.plugkit/plugins/,使默认工具无需去市场下载即可使用。
//!
//! 复制仅发生在插件尚未安装时(幂等)。用户卸载后不会自动恢复。
//! 预装列表动态发现资源目录(有 manifest.json 的子目录),新增内置插件
//! 无需再改常量(此前 download/convert 硬编码列表,playlist 曾漏装)。

use std::path::{Path, PathBuf};
use tauri::Manager;

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

/// 启动时执行预装检查:对资源目录下每个带 manifest.json 的插件,若用户
/// 目录缺失则从资源复制。
pub fn ensure_preinstalled(app: &tauri::AppHandle) {
    let bundled = bundled_plugins_root(app);
    let user = user_plugins_root();

    let Ok(rd) = std::fs::read_dir(&bundled) else {
        log::debug!("No bundled plugins dir at {:?}, skip preinstall", bundled);
        return;
    };
    for entry in rd.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        // 只预装目录且带 manifest.json 的条目(跳过 LICENSE 等杂项文件)
        if !entry.path().is_dir() || !entry.path().join("manifest.json").exists() {
            continue;
        }
        let dst = user.join(&id);
        // 已安装则跳过(含用户主动卸载 → 不恢复)
        if dst.exists() {
            continue;
        }
        // 预装前校验桥接版本(与市场安装同标准,错误前置):
        // 预装插件不经过 install 校验,此处是唯一关卡;不匹配则跳过预装,
        // 避免装入与当前主程序协议不兼容的插件(此前仅运行时才暴露)。
        let manifest_path = entry.path().join("manifest.json");
        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => match serde_json::from_str::<crate::manifest_model::Manifest>(&content) {
                Ok(manifest) => {
                    let ok = crate::bridge_version::check_bridge_version(
                        &manifest.bridge_version,
                        crate::bridge_version::CURRENT_BRIDGE_VERSION,
                    )
                    .unwrap_or(false);
                    if !ok {
                        log::warn!(
                            "Preinstall '{}' skipped: bridge version incompatible (requires {}, app has {})",
                            id, manifest.bridge_version, crate::bridge_version::CURRENT_BRIDGE_VERSION
                        );
                        continue;
                    }
                }
                Err(e) => {
                    log::warn!("Preinstall '{}' skipped: invalid manifest ({})", id, e);
                    continue;
                }
            },
            Err(e) => {
                log::warn!("Preinstall '{}' skipped: read manifest failed ({})", id, e);
                continue;
            }
        }
        match copy_dir(&entry.path(), &dst) {
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
                if let Ok(meta) = std::fs::metadata(&from) {
                    let _ = std::fs::set_permissions(&to, meta.permissions());
                }
            }
        }
    }
    Ok(())
}
