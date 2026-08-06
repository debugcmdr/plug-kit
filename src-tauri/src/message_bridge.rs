use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Message sent by a plugin iframe via postMessage and relayed through the
/// window-level listener into this Tauri command. `command` names one of the
/// plugin bridge operations (plugin invocation, config, task, log).
#[derive(Debug, Clone, Deserialize)]
pub struct BridgeMessage {
    pub id: String,
    pub command: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BridgeResponse {
    pub id: String,
    pub result: Result<serde_json::Value, String>,
}

/// Route an iframe bridge message to the right backend service.
///
/// This is the single entry point the plugin iframes talk to. It keeps the
/// bridge layer fully decoupled from the Vue UI (no business logic in Vue).
pub async fn handle_bridge_message(
    plugin_id: String,
    msg: BridgeMessage,
    app: tauri::AppHandle,
) -> BridgeResponse {
    let result = dispatch(&plugin_id, &msg.command, &msg.payload, &app).await;
    BridgeResponse { id: msg.id, result }
}

async fn dispatch(
    plugin_id: &str,
    command: &str,
    payload: &serde_json::Value,
    app: &tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    match command {
        // Plugin CLI command invocation
        "plugin_command" | "invoke_command" => {
            let command_name = payload
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'command' in payload")?;
            let params = payload.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let runner = crate::tool_runner::ToolRunner::new();
            runner.invoke(plugin_id, command_name, params, Some(app)).await
        }

        // Task queue
        "task_submit" => {
            let task_type = payload
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'type'")?;
            Ok(serde_json::json!({ "task_id": crate::task_queue::TaskQueue::create(plugin_id, task_type).await }))
        }
        "task_cancel" => {
            let task_id = payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'task_id'")?;
            crate::task_queue::TaskQueue::cancel(task_id).await;
            Ok(serde_json::json!({ "cancelled": true }))
        }
        "task_pause" => {
            let task_id = payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'task_id'")?;
            crate::task_queue::TaskQueue::pause(task_id).await;
            Ok(serde_json::json!({ "paused": true }))
        }

        // Config (per-plugin settings)
        "config_get" => {
            let key = payload.get("key").and_then(|v| v.as_str()).ok_or("Missing 'key'")?;
            let cm = crate::config_mgr::ConfigMgr::new();
            Ok(serde_json::to_value(cm.get(plugin_id, key).map_err(|e| e.to_string())?)
                .unwrap_or(serde_json::Value::Null))
        }
        "config_set" => {
            let key = payload.get("key").and_then(|v| v.as_str()).ok_or("Missing 'key'")?;
            let value = payload.get("value").and_then(|v| v.as_str()).ok_or("Missing 'value'")?;
            let cm = crate::config_mgr::ConfigMgr::new();
            cm.set(plugin_id, key, value).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ok": true }))
        }

        // Logging
        "log_info" => {
            let msg = payload.get("msg").and_then(|v| v.as_str()).unwrap_or("");
            crate::logger::Logger::info(msg);
            Ok(serde_json::json!({}))
        }
        "log_error" => {
            let msg = payload.get("msg").and_then(|v| v.as_str()).unwrap_or("");
            crate::logger::Logger::error(msg);
            Ok(serde_json::json!({}))
        }

        // Bridge version handshake
        "bridge_version" => Ok(serde_json::json!({
            "version": crate::bridge_version::CURRENT_BRIDGE_VERSION
        })),

        // 原生文件选择对话框
        "dialog_open_file" => {
            let exts = payload.get("extensions")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|e| e.as_str().map(String::from)).collect::<Vec<_>>());
            let path = crate::dialog_open_file_inner(app.clone(), exts).await?;
            Ok(serde_json::json!({ "path": path }))
        }

        other => Err(format!("Unknown bridge command: {}", other)),
    }
}

/// Emit a progress event to all plugin iframes listening on `plugkit:task-progress`.
pub fn emit_progress(app: &tauri::AppHandle, plugin_id: &str, task_id: &str, data: serde_json::Value) {
    let _ = app.emit("plugkit:task-progress", serde_json::json!({
        "plugin_id": plugin_id,
        "task_id": task_id,
        "data": data,
    }));
}

/// Emit a task state change (for the TaskBar).
pub fn emit_task_updated(app: &tauri::AppHandle, task: &serde_json::Value) {
    let _ = app.emit("plugkit:task-updated", task.clone());
}
