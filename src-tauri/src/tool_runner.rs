use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;

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

    pub async fn invoke(
        &self,
        plugin_id: &str,
        command_name: &str,
        params: Value,
    ) -> Result<Value, String> {
        let plugin_dir = self.plugin_dir(plugin_id);
        let tool_dir = plugin_dir.join("tool");

        // Find executable
        let exe = Self::find_executable(&tool_dir)?;

        // Read manifest to get command args
        let manifest = self.read_manifest(plugin_id)?;
        let command = manifest.commands.get(command_name)
            .ok_or_else(|| format!("Command '{}' not found in manifest", command_name))?;

        // Build args with placeholder substitution
        let args = self.substitute_args(&command.stdin_args, &params);

        // Spawn process
        let mut child = Command::new(&exe)
            .args(&args)
            .current_dir(&self.work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to take stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to take stderr")?;

        let stdout_handle = tokio::spawn(Self::read_stdout(stdout));
        let stderr_handle = tokio::spawn(Self::read_stderr(stderr));

        let (stdout_res, stderr_res) = tokio::join!(stdout_handle, stderr_handle);
        let stdout_lines = stdout_res
            .map_err(|e| format!("stdout task failed: {}", e))?
            .map_err(|e| e.to_string())?;
        let _stderr_output = stderr_res
            .map_err(|e| format!("stderr task failed: {}", e))?
            .map_err(|e| e.to_string())?;

        // Parse last line as result
        let last_line = stdout_lines.last()
            .ok_or("No output from plugin")?
            .clone();

        match serde_json::from_str::<PluginOutput>(&last_line) {
            Ok(PluginOutput::Result(r)) => Ok(r.data),
            Ok(PluginOutput::Error(e)) => Err(e.message),
            Ok(_) => Ok(serde_json::from_str(&last_line).unwrap_or(Value::Null)),
            Err(_) => Ok(serde_json::json!({"raw_output": last_line})),
        }
    }

    fn find_executable(tool_dir: &PathBuf) -> Result<PathBuf, String> {
        for entry in std::fs::read_dir(tool_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext != "exe" && ext != "dll" {
                        return Ok(path);
                    }
                } else if cfg!(target_os = "windows") {
                    // On Windows, try without extension
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

    fn substitute_args(&self, args: &[String], params: &Value) -> Vec<String> {
        args.iter()
            .map(|arg| {
                let mut result = arg.clone();
                if let Value::Object(map) = params.clone() {
                    for (key, value) in &map {
                        let placeholder = format!("{{{}}}", key);
                        result = result.replace(&placeholder, &value.to_string());
                    }
                }
                result
            })
            .collect()
    }

    async fn read_stdout(
        reader: impl AsyncRead + Unpin + Send + 'static,
    ) -> Result<Vec<String>, String> {
        let mut reader = BufReader::new(reader);
        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    let line = line.trim().to_string();
                    if !line.is_empty() {
                        lines.push(line);
                    }
                }
                Err(e) => return Err(format!("Failed to read stdout: {}", e)),
            }
        }
        Ok(lines)
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
