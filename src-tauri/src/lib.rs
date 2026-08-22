mod security;
mod manifest_model;
mod manifest_fetcher;
mod message_bridge;
mod protocol;
mod preinstall;
mod dep_manifest;
mod tool_runner;
mod task_queue;
mod task_registry;
mod dependency_cache;
mod plugin_manager;
mod config_mgr;
mod logger;
mod bridge_version;
mod registry;

/// Public API facade — used by integration tests and future CLI/headless tooling.
pub mod api {
    pub use crate::manifest_fetcher::{download_with_fallback, fetch_market_manifests, bundled_manifests};
    pub use crate::dependency_cache::DependencyCache;
    pub use crate::config_mgr::ConfigMgr;
    pub use crate::manifest_model::{Manifest, CommandDefinition};
    pub use crate::security::{validate_unzip_path, validate_sha256};
    pub use crate::plugin_manager::PluginManager;
    pub use crate::tool_runner::ToolRunner;
}

use serde::{Deserialize, Serialize};

/// 模块级单例：PluginManager 持有插件安装/卸载/列表状态，避免每个命令重建实例。
static PLUGIN_MANAGER: once_cell::sync::Lazy<plugin_manager::PluginManager> =
    once_cell::sync::Lazy::new(plugin_manager::PluginManager::new);

/// 模块级单例：ToolRunner 持有工作目录配置，避免每次 CLI 调用都重建。
static TOOL_RUNNER: once_cell::sync::Lazy<tool_runner::ToolRunner> =
    once_cell::sync::Lazy::new(tool_runner::ToolRunner::new);

/// Bridge entry point: plugin iframes send postMessage -> window listener ->
/// this command -> dispatched to the right backend service.
#[tauri::command]
async fn bridge_message(
    app: tauri::AppHandle,
    plugin_id: String,
    msg: message_bridge::BridgeMessage,
) -> message_bridge::BridgeResponse {
    message_bridge::handle_bridge_message(plugin_id, msg, app).await
}

/// 原生文件选择对话框核心逻辑(命令与 bridge 共用)。
/// `extensions`: 允许的扩展名列表(如 ["mp4","mkv"]),空则全部。
/// `multiple`: 是否允许多选。
/// 返回选中文件路径列表(多选),取消返回空列表。
pub async fn dialog_open_file_inner(
    app: tauri::AppHandle,
    extensions: Option<Vec<String>>,
    multiple: bool,
) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    // 构造对话框:按扩展名过滤
    let mut builder = app.dialog().file();
    if let Some(exts) = &extensions {
        if !exts.is_empty() {
            let exts: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            builder = builder.add_filter("支持的文件", &exts);
        }
    }

    // blocking 同步等待用户选择(模态阻塞,无超时/悬空问题)
    let result = if multiple {
        builder.blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string())
            .collect::<Vec<String>>()
    } else {
        match builder.blocking_pick_file() {
            Some(p) => vec![p.to_string()],
            None => vec![],
        }
    };
    Ok(result)
}

/// 选择文件夹(供插件 iframe 调用,如"更改保存路径")。
/// 返回选中的文件夹路径,取消返回 None。
pub async fn dialog_open_folder_inner(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

/// 在系统文件管理器中打开指定路径(文件夹或文件的所在目录)。
/// 供插件 iframe 的「打开保存路径」按钮调用。
#[tauri::command]
async fn open_in_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    // 支持 ~/ 前缀展开(插件 UI 的默认保存位置形如 ~/Downloads/PlugKit)
    let expanded = if path.starts_with("~/") {
        dirs::home_dir()
            .map(|h| h.join(&path[2..]))
            .unwrap_or_else(|| std::path::PathBuf::from(&path))
    } else {
        std::path::PathBuf::from(&path)
    };
    let p = expanded;
    // 空/无效路径:直接报错,绝不打开进程工作目录(cwd)——否则用户会莫名跳转到 src-tauri。
    if p.as_os_str().is_empty() {
        return Err("打开路径为空".into());
    }
    let target = if p.is_dir() {
        p
    } else if let Some(parent) = p.parent() {
        parent.to_path_buf()
    } else {
        p
    };
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(target.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// 返回用户主目录路径(供前端打开数据/日志目录使用)。
#[tauri::command]
fn get_home_dir() -> String {
    dirs::home_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CommandOutput {
    Json,
    ProgressJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub is_installed: bool,
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub success: bool,
    pub plugin_id: String,
    pub message: String,
    /// 结构化错误码(前端可据此区分场景而非解析 message 文本):
    /// ALREADY_LATEST / DOWNGRADE_BLOCKED / BRIDGE_INCOMPATIBLE /
    /// MIN_APP_VERSION / UNSIGNED_PLUGIN / INSTALL_OK 等。
    #[serde(default)]
    pub code: Option<String>,
}

#[tauri::command]
async fn install_plugin(plugin_id: String) -> Result<InstallResult, String> {
    PLUGIN_MANAGER.install(&plugin_id).await
}

#[tauri::command]
async fn uninstall_plugin(plugin_id: String) -> Result<InstallResult, String> {
    PLUGIN_MANAGER.uninstall(&plugin_id).await
}

#[tauri::command]
async fn list_plugins() -> Result<Vec<PluginInfo>, String> {
    PLUGIN_MANAGER.list().await
}

/// Market view: installed plugins (from disk) + available plugins (from marketplace manifest).
/// Each entry carries `is_installed` / `installed_version` so the UI can toggle install state.
#[tauri::command]
async fn market_list() -> Result<Vec<PluginInfo>, String> {
    let installed = PLUGIN_MANAGER.list().await?;
    let available = manifest_fetcher::fetch_market_manifests().await?;

    let mut merged: Vec<PluginInfo> = Vec::new();
    for m in available {
        let existing = installed.iter().find(|p| p.id == m.id);
        merged.push(PluginInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            is_installed: existing.is_some(),
            installed_version: existing.map(|p| p.version.clone()),
        });
    }
    // Include any installed plugins not present in the market manifest.
    for p in installed {
        if !merged.iter().any(|m| m.id == p.id) {
            merged.push(p);
        }
    }
    Ok(merged)
}

/// Force-refresh the remote marketplace manifest cache.
#[tauri::command]
async fn market_refresh() -> Result<(), String> {
    manifest_fetcher::invalidate_market_cache();
    manifest_fetcher::fetch_market_manifests().await.map(|_| ())
}

#[tauri::command]
fn get_cache_stats() -> Result<serde_json::Value, String> {
    let dc = dependency_cache::DependencyCache::new();
    dc.stats()
}

#[tauri::command]
async fn clean_orphan_cache() -> Result<serde_json::Value, String> {
    let dc = dependency_cache::DependencyCache::new();
    dc.clean_orphans().await
}

#[tauri::command]
fn get_plugin_config(plugin_id: String, key: String) -> Result<Option<String>, String> {
    let cm = config_mgr::ConfigMgr::new();
    cm.get(&plugin_id, &key).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_plugin_config(plugin_id: String, key: String, value: String) -> Result<(), String> {
    let cm = config_mgr::ConfigMgr::new();
    cm.set(&plugin_id, &key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_bridge_version() -> String {
    bridge_version::CURRENT_BRIDGE_VERSION.to_string()
}

#[tauri::command]
fn check_bridge_compat(required: String) -> Result<bool, String> {
    bridge_version::check_bridge_version(&required, bridge_version::CURRENT_BRIDGE_VERSION)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_tasks() -> Result<Vec<serde_json::Value>, String> {
    Ok(task_queue::TaskQueue::list().await)
}

#[tauri::command]
fn log_info(msg: String) {
    logger::Logger::info(&msg);
}

#[tauri::command]
fn log_error(msg: String) {
    logger::Logger::error(&msg);
}

/// 读取最近 N 行日志内容(用于日志查看器)。
/// `lines`: 读取行数,默认 200,最大 2000。
/// `level`: 过滤级别,空则全部。
#[tauri::command]
async fn log_read(lines: Option<usize>, level: Option<String>) -> Result<Vec<String>, String> {
    use std::io::{BufRead, BufReader};
    let n = lines.unwrap_or(200).min(2000);
    let level_filter = level.as_deref();

    let log_dir = dirs::home_dir()
        .ok_or("无法获取用户目录")?
        .join(".plugkit")
        .join("logs");

    // 按文件名降序取最近的日志文件
    let mut files: Vec<std::path::PathBuf> = if log_dir.is_dir() {
        std::fs::read_dir(&log_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "log"))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    let mut all_lines: Vec<String> = Vec::new();
    for file in files {
        let reader = BufReader::new(
            std::fs::File::open(&file).map_err(|e| format!("打开日志文件: {}", e))?
        );
        let mut lines: Vec<String> = reader.lines().collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        lines.reverse();
        for line in lines {
            if line.is_empty() { continue; }
            if let Some(lv) = level_filter {
                if !line.starts_with(&format!("[{}]", lv)) { continue; }
            }
            all_lines.push(line);
            if all_lines.len() >= n { break; }
        }
        if all_lines.len() >= n { break; }
    }
    all_lines.reverse();
    Ok(all_lines)
}

pub fn run() {
    env_logger::init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 启动时预装默认工具(尚未安装时)
            preinstall::ensure_preinstalled(app.handle());
            // 上次运行遗留的非终态任务标记为 interrupted(子进程已随退出消失)
            tauri::async_runtime::spawn(async move {
                task_queue::TaskQueue::recover().await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            install_plugin,
            uninstall_plugin,
            list_plugins,
            market_list,
            market_refresh,
            bridge_message,
            open_in_folder,
            get_home_dir,
            get_cache_stats,
            clean_orphan_cache,
            get_plugin_config,
            set_plugin_config,
            get_bridge_version,
            check_bridge_compat,
            list_tasks,
            log_info,
            log_error,
            log_read,
        ]);
    protocol::register_protocol(builder)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // 应用退出:把仍在运行/排队/暂停的任务标记为 interrupted(落盘),
            // 避免任务卡在 running 直到下次启动才被 recover。
            if let tauri::RunEvent::Exit = event {
                let _ = tauri::async_runtime::block_on(task_queue::TaskQueue::recover());
                let _ = app_handle;
            }
        });
}
