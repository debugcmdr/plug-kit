use serde::{Deserialize, Serialize};

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

/// 从插件调用参数中提取来源 URL(提取/下载类命令的 `{ "url": ... }`)。
fn extract_url(params: &serde_json::Value) -> Option<String> {
    params.get("url").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// 重复提取时的提示文案(去重仅命中活跃任务;终态任务允许重新解析)。
fn dedup_message() -> String {
    "该链接正在解析中，请勿重复提取".to_string()
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

/// 校验插件 manifest 是否声明了该命令。
/// 未知命令(拼错的内置命令/历史死 API)不应创建任务——直接报错返回,
/// 避免任务中心堆积无意义 failed 记录(旧兜底分支对任何命令名都先建任务)。
fn plugin_has_command(plugin_id: &str, command_name: &str) -> bool {
    let path = dirs::home_dir()
        .map(|d| d.join(".plugkit/plugins").join(plugin_id).join("manifest.json"))
        .unwrap_or_default();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<crate::manifest_model::Manifest>(&content) else {
        return false;
    };
    manifest.commands.contains_key(command_name)
}

/// 插件 CLI 命令调用统一入口(invoke_command 与兜底分支共用,消除重复)。
/// 任务中心闭环:无 task_id 时先创建任务,再把同一 id 透传给 ToolRunner。
/// (此前 ToolRunner 内部另起 id,导致任务中心永远 pending、取消杀不掉进程。)
async fn invoke_cli(
    plugin_id: &str,
    command_name: &str,
    params: serde_json::Value,
    payload: &serde_json::Value,
    app: &tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    if !plugin_has_command(plugin_id, command_name) {
        return Err(format!(
            "Unknown command '{}' (not declared in plugin '{}' manifest)",
            command_name, plugin_id
        ));
    }
    let task_id = match payload.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            let url = extract_url(&params);
            // 非 download 命令:原子查重+创建(锁内检查,并发同 URL 只建一个任务,
            // 防 TOCTOU 穿透;终态任务允许重新解析)。download 允许重复下载。
            if command_name == "download" {
                crate::task_queue::TaskQueue::create(plugin_id, command_name, url).await
            } else {
                match crate::task_queue::TaskQueue::create_with_dedup(plugin_id, command_name, url).await {
                    Some(id) => id,
                    None => return Err(dedup_message()),
                }
            }
        }
    };
    crate::TOOL_RUNNER.invoke(plugin_id, command_name, params, Some(app), Some(task_id)).await
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
            invoke_cli(plugin_id, command_name, params, payload, app).await
        }

        // Task queue
        // 任务创建统一由 invoke_command / 兜底分支完成(MT.invoke 自动建任务,
        // 含 URL 去重);任务控制命令供插件管理自己的任务。
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
        "task_resume" => {
            let task_id = payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'task_id'")?;
            crate::task_queue::TaskQueue::resume(task_id).await;
            Ok(serde_json::json!({ "resumed": true }))
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
            let multiple = payload.get("multiple").and_then(|v| v.as_bool()).unwrap_or(false);
            let paths = crate::dialog_open_file_inner(app.clone(), exts, multiple).await?;
            Ok(serde_json::json!({ "paths": paths }))
        }
        // 选择文件夹(更改保存路径)
        "dialog_open_folder" => {
            let path = crate::dialog_open_folder_inner(app.clone()).await?;
            Ok(serde_json::json!({ "path": path }))
        }
        // 在系统文件管理器中打开路径
        "open_in_folder" => {
            let path = payload.get("path").and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?;
            crate::open_in_folder(app.clone(), path.to_string()).await?;
            Ok(serde_json::json!({ "opened": true }))
        }

        // macOS 系统权限设置:打开「完全磁盘访问」面板(浏览器 cookies 授权用,
        // 与 CLI 诊断的 COOKIES_PERMISSION 错误配套,一键直达授权页)。
        "open_permission_settings" => {
            crate::open_permission_settings().await?;
            Ok(serde_json::json!({ "opened": true }))
        }

        // 兜底:任何未识别的命令名都当作插件 CLI 命令直接调用
        // (插件作者可 MT.invoke('download', {...}) 直观调用 manifest 命令)。
        // 命令存在性由 invoke_cli 前置校验:未知命令报错且不创建任务。
        other => {
            let params = payload.get("params").cloned()
                .unwrap_or_else(|| payload.clone());
            invoke_cli(plugin_id, other, params, payload, app).await
        }
    }
}
