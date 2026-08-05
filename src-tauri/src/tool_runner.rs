use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginOutput {
    Progress(ProgressData),
    Result(ResultData),
    Error(ErrorData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressData {
    pub percent: Option<f64>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultData {
    pub success: bool,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    pub code: String,
    pub message: String,
}

pub struct ToolRunner {
    work_dir: PathBuf,
}

impl ToolRunner {
    pub fn new() -> Self {
        Self {
            work_dir: dirs::home_dir()
                .map(|d| d.join(".plugkit/tmp"))
                .unwrap_or_default(),
        }
    }

    /// Invoke a plugin CLI command.
    ///
    /// * `output_mode == "progress+json"` — stream each `progress` line via
    ///   `plugkit:task-progress` events; the final `result`/`error` line is returned.
    /// * `output_mode == "json"` — parse a single result line.
    ///
    /// `cancel_flag` (when Some) is checked each line; setting it kills the child.
    pub async fn invoke(
        &self,
        plugin_id: &str,
        command_name: &str,
        params: Value,
        app: Option<&tauri::AppHandle>,
    ) -> Result<Value, String> {
        let plugin_dir = self.plugin_dir(plugin_id);
        let manifest = self.read_manifest(plugin_id)?;
        let command = manifest.commands.get(command_name)
            .ok_or_else(|| format!("Command '{}' not found in manifest", command_name))?;

        let exe = Self::find_executable(&plugin_dir.join(manifest.tool_dir()))?;
        let args = self.substitute_args(&command.stdin_args, &params, &manifest);

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let mut child = Command::new(&exe)
            .args(&args)
            .current_dir(&self.work_dir)
            .envs(self.dependency_envs(&manifest))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to take stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to take stderr")?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let cancel = cancel_flag.clone();
        let app_clone = app.map(|a| a.clone());

        let stdout_task = tokio::spawn(Self::read_stdout_protocol(
            stdout,
            plugin_id.to_string(),
            task_id.clone(),
            app_clone,
            cancel,
            command.output_mode.clone(),
        ));

        let stderr_task = tokio::spawn(Self::read_stderr(stderr));

        // Wait for the process while allowing cancel / timeout.
        let is_progress = command.output_mode == "progress+json";
        let outcome = Self::await_with_cancel(&mut child, &cancel_flag, is_progress).await;

        if let Some(flag) = outcome.killed {
            // Cancelled / timed out
            let _ = child.kill().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(flag);
        }

        let (stdout_res, stderr_res) = tokio::join!(stdout_task, stderr_task);
        let lines = stdout_res.map_err(|e| format!("stdout task failed: {}", e))?;
        let _stderr = stderr_res.map_err(|e| format!("stderr task failed: {}", e))?;

        if !outcome.success && !lines.iter().any(|l| l.contains("\"type\":\"error\"")) {
            return Err(format!("Plugin exited with code {}", outcome.code));
        }

        // Final line carries the result.
        let last = lines.into_iter().rev().find(|l| !l.is_empty())
            .ok_or("No output from plugin")?;

        match serde_json::from_str::<PluginOutput>(&last) {
            Ok(PluginOutput::Result(r)) => Ok(r.data),
            Ok(PluginOutput::Error(e)) => Err(e.message),
            Ok(_) => Ok(serde_json::from_str(&last).unwrap_or(Value::Null)),
            Err(_) => Ok(serde_json::json!({ "raw_output": last })),
        }
    }

    /// Wait for the child to exit, watching the cancel flag when in progress mode.
    async fn await_with_cancel(
        child: &mut Child,
        cancel_flag: &Arc<AtomicBool>,
        watch_cancel: bool,
    ) -> ProcessOutcome {
        loop {
            // If cancellation was requested, stop waiting.
            if watch_cancel && cancel_flag.load(Ordering::SeqCst) {
                return ProcessOutcome { success: false, code: -1, killed: Some("Task cancelled".into()) };
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return ProcessOutcome {
                        success: status.success(),
                        code: status.code().unwrap_or(-1),
                        killed: None,
                    };
                }
                Ok(None) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return ProcessOutcome {
                    success: false,
                    code: -1,
                    killed: Some(format!("Wait error: {}", e)),
                },
            }
        }
    }

    fn dependency_envs(&self, manifest: &crate::manifest_model::Manifest) -> Vec<(String, String)> {
        let mut envs = Vec::new();
        if let Some(deps) = &manifest.dependencies {
            for name in deps.keys() {
                match name.to_lowercase().as_str() {
                    "ffmpeg" => envs.push(("PLUGKIT_FFMPEG".into(), "ffmpeg".into())),
                    "yt-dlp" => envs.push(("PLUGKIT_YTDLP".into(), "yt-dlp".into())),
                    _ => {}
                }
            }
        }
        envs
    }

    fn find_executable(tool_dir: &PathBuf) -> Result<PathBuf, String> {
        for entry in std::fs::read_dir(tool_dir).map_err(|e| format!("Read tool dir: {}", e))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext != "exe" && ext != "dll" {
                        return Ok(path);
                    }
                } else if cfg!(target_os = "windows") {
                    return Ok(path);
                } else if !path.to_string_lossy().contains('.') {
                    return Ok(path);
                }
            }
        }
        Err("No executable found in tool directory".to_string())
    }

    fn read_manifest(&self, plugin_id: &str) -> Result<crate::manifest_model::Manifest, String> {
        let manifest_path = self.plugin_dir(plugin_id).join("manifest.json");
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse manifest: {}", e))
    }

    fn plugin_dir(&self, plugin_id: &str) -> PathBuf {
        dirs::home_dir()
            .map(|d| d.join(".plugkit/plugins").join(plugin_id))
            .unwrap_or_default()
    }

    /// Substitute `{var}` placeholders from params, plus `{platform}`.
    fn substitute_args(&self, args: &[String], params: &Value, _manifest: &crate::manifest_model::Manifest) -> Vec<String> {
        let platform = if cfg!(all(target_os = "windows", target_arch = "x86_64")) { "windows-x64" }
            else if cfg!(all(target_os = "macos", target_arch = "aarch64")) { "macos-arm64" }
            else if cfg!(all(target_os = "macos", target_arch = "x86_64")) { "macos-x64" }
            else if cfg!(all(target_os = "linux", target_arch = "x86_64")) { "linux-x64" }
            else { "unknown" };

        args.iter()
            .map(|arg| {
                let mut result = arg.clone();
                result = result.replace("{platform}", platform);
                if let Value::Object(map) = params {
                    for (key, value) in map {
                        let placeholder = format!("{{{}}}", key);
                        let value_str = match value {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        result = result.replace(&placeholder, &value_str);
                    }
                }
                result
            })
            .collect()
    }

    /// Read stdout line by line, emitting progress events and capturing result/error lines.
    async fn read_stdout_protocol(
        reader: impl AsyncRead + Unpin + Send + 'static,
        plugin_id: String,
        task_id: String,
        app: Option<tauri::AppHandle>,
        cancel: Arc<AtomicBool>,
        output_mode: String,
    ) -> Vec<String> {
        let mut reader = BufReader::new(reader);
        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if output_mode == "progress+json" {
                        if let Ok(output) = serde_json::from_str::<PluginOutput>(&trimmed) {
                            if let PluginOutput::Progress(p) = output {
                                if let Some(app) = &app {
                                    let _ = app.emit("plugkit:task-progress", serde_json::json!({
                                        "plugin_id": plugin_id,
                                        "task_id": task_id,
                                        "data": serde_json::to_value(&p).unwrap_or(Value::Null),
                                    }));
                                }
                            }
                        }
                    }
                    lines.push(trimmed);
                }
                Err(_) => break,
            }
        }
        lines
    }

    async fn read_stderr(
        reader: impl AsyncRead + Unpin + Send + 'static,
    ) -> Result<String, String> {
        let mut reader = BufReader::new(reader);
        let mut output = String::new();
        reader.read_to_string(&mut output).await
            .map_err(|e| format!("Failed to read stderr: {}", e))?;
        Ok(output)
    }
}

struct ProcessOutcome {
    success: bool,
    code: i32,
    killed: Option<String>,
}
