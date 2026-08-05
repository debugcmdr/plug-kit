mod security;
mod manifest_model;
mod manifest_fetcher;
mod message_bridge;
mod protocol;
mod tool_runner;
mod task_queue;
mod dependency_cache;
mod plugin_manager;
mod config_mgr;
mod logger;
mod bridge_version;
mod registry;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

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
}

#[tauri::command]
async fn install_plugin(plugin_id: String, _release_url: Option<String>) -> Result<InstallResult, String> {
    let pm = plugin_manager::PluginManager::new();
    pm.install(&plugin_id).await
}

#[tauri::command]
async fn uninstall_plugin(plugin_id: String) -> Result<InstallResult, String> {
    let pm = plugin_manager::PluginManager::new();
    pm.uninstall(&plugin_id).await
}

#[tauri::command]
async fn list_plugins() -> Result<Vec<PluginInfo>, String> {
    let pm = plugin_manager::PluginManager::new();
    pm.list().await
}

/// Market view: installed plugins (from disk) + available plugins (from marketplace manifest).
/// Each entry carries `is_installed` / `installed_version` so the UI can toggle install state.
#[tauri::command]
async fn market_list() -> Result<Vec<PluginInfo>, String> {
    let pm = plugin_manager::PluginManager::new();
    let installed = pm.list().await?;
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
fn get_settings() -> Result<serde_json::Value, String> {
    let cm = config_mgr::ConfigMgr::new();
    cm.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_settings(settings: serde_json::Value) -> Result<(), String> {
    let cm = config_mgr::ConfigMgr::new();
    cm.set_settings(settings).map_err(|e| e.to_string())
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

pub fn run() {
    env_logger::init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            install_plugin,
            uninstall_plugin,
            list_plugins,
            market_list,
            market_refresh,
            bridge_message,
            get_cache_stats,
            clean_orphan_cache,
            get_settings,
            set_settings,
            get_plugin_config,
            set_plugin_config,
            get_bridge_version,
            check_bridge_compat,
            list_tasks,
            log_info,
            log_error,
        ]);
    protocol::register_protocol(builder)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
