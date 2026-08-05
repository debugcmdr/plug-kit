mod security;
mod manifest_model;
mod message_bridge;
mod tool_runner;
mod task_queue;
mod dependency_cache;
mod plugin_manager;
mod config_mgr;
mod logger;
mod bridge_version;
mod registry;

use serde::{Deserialize, Serialize};

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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            install_plugin,
            uninstall_plugin,
            list_plugins,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
