use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

/// 最大并发任务数:第 N+1 个任务排队等待(任务中心显示 pending)。
const MAX_CONCURRENT_TASKS: usize = 5;

/// 全局并发信号量(令牌数 = 并发上限)。
static TASK_SEMAPHORE: once_cell::sync::Lazy<tokio::sync::Semaphore> =
    once_cell::sync::Lazy::new(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_TASKS));

/// 轮询等待 cancel 标志被置位(用于排队等待并发名额期间响应取消)。
async fn wait_cancelled(flag: &Arc<AtomicBool>) {
    loop {
        if flag.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// 暂停子进程:Unix 对进程组发 SIGSTOP(连带 ffmpeg/yt-dlp 子进程),
/// Windows 用 taskkill /suspend。暂停的是整个任务,恢复后从原进度继续。
fn suspend_process(child: &Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-STOP", &format!("-{}", pid)])
            .status();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/suspend", "/pid", &pid.to_string()])
            .status();
    }
}

/// 恢复子进程:Unix 对进程组发 SIGCONT,Windows 用 taskkill /resume。
fn resume_process(child: &Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-CONT", &format!("-{}", pid)])
            .status();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/resume", "/pid", &pid.to_string()])
            .status();
    }
}

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
    /// 转换文件名/输出路径(插件 progress 上报,任务中心展示与"打开输出文件夹")。
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
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
    /// `task_id` (when Some) is the task center's id: it is used to register the
    /// control flags (cancel / pause), and task status/progress are mirrored into
    /// the task center. When None (tests / headless), a fresh id is generated but
    /// not persisted.
    ///
    /// 并发上限 5(Semaphore 排队,排队中可取消);Unix 下子进程放入独立进程组,
    /// 暂停/恢复对进程组发 SIGSTOP/SIGCONT,可作用于 ffmpeg/yt-dlp 等子进程。
    pub async fn invoke(
        &self,
        plugin_id: &str,
        command_name: &str,
        params: Value,
        app: Option<&tauri::AppHandle>,
        task_id: Option<String>,
    ) -> Result<Value, String> {
        let plugin_dir = self.plugin_dir(plugin_id);
        let manifest = self.read_manifest(plugin_id)?;
        let command = manifest.commands.get(command_name)
            .ok_or_else(|| format!("Command '{}' not found in manifest", command_name))?;

        let exe = Self::find_executable(&plugin_dir.join(manifest.tool_dir()), &manifest)?;
        let args = self.substitute_args(&command.stdin_args, &params, &manifest);

        // 任务 ID：bridge 层创建后透传；独立调用(测试/无 UI 场景)则内部生成。
        let task_id = task_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        // 注册控制标志(提前到并发排队之前:排队中的任务也能被取消/暂停)。
        let control = crate::task_registry::register(&task_id).await;
        let cancel_flag = control.cancel.clone();
        let pause_flag = control.pause.clone();

        // 并发上限(MAX_CONCURRENT_TASKS):排队等待名额,排队期间可取消。
        let _permit = tokio::select! {
            permit = TASK_SEMAPHORE.acquire() => match permit {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!("Task queue acquire failed: {}", e);
                    crate::task_registry::unregister(&task_id).await;
                    crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
                    log::error!("[{}] {}", task_id, msg);
                    return Err(msg);
                }
            },
            _ = wait_cancelled(&cancel_flag) => {
                crate::task_registry::unregister(&task_id).await;
                crate::task_queue::TaskQueue::update_status(&task_id, crate::task_queue::TaskStatus::Cancelled).await;
                return Err("Task cancelled while queued".to_string());
            }
        }; // _permit 持有到函数结束 = 占用并发名额

        // 任务状态:running;若在排队期间已被暂停则保持 paused。
        let start_paused = pause_flag.load(Ordering::SeqCst);
        crate::task_queue::TaskQueue::update_status(
            &task_id,
            if start_paused {
                crate::task_queue::TaskStatus::Paused
            } else {
                crate::task_queue::TaskStatus::Running
            },
        ).await;
        // 每个任务独立工作目录:避免多任务共享 ~/.plugkit/tmp 互相污染。
        let work_dir = self.work_dir.join(plugin_id).join(&task_id);
        if let Err(e) = std::fs::create_dir_all(&work_dir) {
            let msg = format!("Create work dir: {}", e);
            crate::task_registry::unregister(&task_id).await;
            crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
            log::error!("[{}] {}", task_id, msg);
            let _ = std::fs::remove_dir_all(&work_dir);
            return Err(msg);
        }

        // 依赖准备阶段:对任务中心透明展示「正在准备依赖」,而非停留在「等待中」黑盒。
        // 此阶段在解析计时之外——插件 info() 的 90s 超时只计 yt-dlp 解析时长,不含依赖下载。
        crate::task_queue::TaskQueue::update_progress(
            &task_id, 0.0, None, None,
            Some("正在准备共享依赖（yt-dlp）…".to_string()),
            None, None,
        ).await;
        // 解析共享依赖真实路径(缓存优先;ffmpeg 系统 PATH 优先)。
        // 彻底不可用时任务直接失败并给出明确原因,不再进入插件报笼统的 YTDLP_MISSING。
        let dep_envs = match self.resolve_dependency_envs(plugin_id, &manifest).await {
            Ok(envs) => envs,
            Err(e) => {
                crate::task_registry::unregister(&task_id).await;
                crate::task_queue::TaskQueue::update_status_with_error(
                    &task_id, crate::task_queue::TaskStatus::Failed, Some(e.clone()),
                ).await;
                log::error!("[{}] {}", task_id, e);
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(e);
            }
        };

        // spawn(Unix 下放入独立进程组,暂停/恢复作用于整个任务树)。
        let mut cmd = Command::new(&exe);
        cmd.args(&args)
            .current_dir(&work_dir)
            .envs(dep_envs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // 应用退出/Child 被 drop 时自动杀子进程,避免留下孤儿 ffmpeg/yt-dlp。
        cmd.kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Failed to spawn plugin '{}': {}", exe.display(), e);
                crate::task_registry::unregister(&task_id).await;
                crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
                log::error!("[{}] {}", task_id, msg);
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(msg);
            }
        };

        let stdout = child.stdout.take().ok_or("Failed to take stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to take stderr")?;

        let app_clone = app.map(|a| a.clone());
        let stdout_task = tokio::spawn(Self::read_stdout_protocol(
            stdout,
            plugin_id.to_string(),
            task_id.clone(),
            app_clone,
            cancel_flag.clone(),
            command.output_mode.clone(),
        ));

        let stderr_task = tokio::spawn(Self::read_stderr(stderr));

        // Wait for the process while allowing cancel / pause / timeout.
        let is_progress = command.output_mode == "progress+json";
        let outcome = Self::await_with_cancel(&mut child, &cancel_flag, &pause_flag, is_progress).await;
        if let Some(flag) = outcome.killed {
            // Cancelled / timed out
            let _ = child.kill().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            crate::task_registry::unregister(&task_id).await;
            crate::task_queue::TaskQueue::update_status(&task_id, crate::task_queue::TaskStatus::Cancelled).await;
            let _ = std::fs::remove_dir_all(&work_dir);
            return Err(flag);
        }

        let (stdout_res, stderr_res) = tokio::join!(stdout_task, stderr_task);
        let lines = match stdout_res {
            Ok(l) => l,
            Err(e) => {
                let msg = format!("stdout task failed: {}", e);
                crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
                crate::task_registry::unregister(&task_id).await;
                log::error!("[{}] {}", task_id, msg);
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(msg);
            }
        };
        if let Err(e) = stderr_res {
            let msg = format!("stderr task failed: {}", e);
            crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
            crate::task_registry::unregister(&task_id).await;
            log::error!("[{}] {}", task_id, msg);
            let _ = std::fs::remove_dir_all(&work_dir);
            return Err(msg);
        }
        crate::task_registry::unregister(&task_id).await;

        if !outcome.success && !lines.iter().any(|l| l.contains("\"type\":\"error\"")) {
            let msg = format!("Plugin exited with code {}", outcome.code);
            crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
            log::error!("[{}] {}", task_id, msg);
            let _ = std::fs::remove_dir_all(&work_dir);
            return Err(msg);
        }

        // Final line carries the result.
        let last = match lines.into_iter().rev().find(|l| !l.is_empty()) {
            Some(l) => l,
            None => {
                let msg = "Plugin produced no output on stdout".to_string();
                crate::task_queue::TaskQueue::update_status_with_error(&task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone())).await;
                log::error!("[{}] {}", task_id, msg);
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(msg);
            }
        };

        let final_outcome = serde_json::from_str::<PluginOutput>(&last);
        match &final_outcome {
            Ok(PluginOutput::Error(e)) => {
                crate::task_queue::TaskQueue::update_status_with_error(
                    &task_id,
                    crate::task_queue::TaskStatus::Failed,
                    Some(e.message.clone()),
                ).await;
                log::error!("[{}] plugin error: {}", task_id, e.message);
            }
            _ => {
                crate::task_queue::TaskQueue::update_status(&task_id, crate::task_queue::TaskStatus::Completed).await;
            }
        }
        let _ = std::fs::remove_dir_all(&work_dir);

        match final_outcome {
            Ok(PluginOutput::Result(r)) => Ok(r.data),
            Ok(PluginOutput::Error(e)) => Err(e.message),
            Ok(_) => Ok(serde_json::from_str(&last).unwrap_or(Value::Null)),
            Err(_) => Ok(serde_json::json!({ "raw_output": last })),
        }
    }

    /// Wait for the child to exit, watching cancel / pause flags.
    /// 取消(cancel)恒检查:json 模式命令(list-formats 等)也可被任务中心取消,
    /// 否则网络卡顿时任务永久 running 直到子进程自然退出。
    /// 暂停(pause)仅 progress 模式开启(watch_pause)——短命令暂停无意义。
    async fn await_with_cancel(
        child: &mut Child,
        cancel_flag: &Arc<AtomicBool>,
        pause_flag: &Arc<AtomicBool>,
        watch_pause: bool,
    ) -> ProcessOutcome {
        loop {
            // If cancellation was requested, stop waiting (始终检查).
            if cancel_flag.load(Ordering::SeqCst) {
                return ProcessOutcome { success: false, code: -1, killed: Some("Task cancelled".into()) };
            }
            // Pause requested: suspend the process (and its group), then wait for resume.
            if watch_pause && pause_flag.load(Ordering::SeqCst) {
                suspend_process(child);
                while pause_flag.load(Ordering::SeqCst) {
                    if cancel_flag.load(Ordering::SeqCst) {
                        // 先恢复再取消:SIGKILL 对被 STOP 的进程无效
                        resume_process(child);
                        return ProcessOutcome { success: false, code: -1, killed: Some("Task cancelled".into()) };
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                resume_process(child);
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

    /// 解析插件声明依赖为真实可执行路径并注入环境变量。
    /// key 规则:PLUGKIT_{name.upper().replace('-', '_')}(与插件 dep() 一致)。
    /// 优先系统 PATH(ffmpeg),否则用共享缓存路径。
    async fn resolve_dependency_envs(
        &self,
        plugin_id: &str,
        manifest: &crate::manifest_model::Manifest,
    ) -> Result<Vec<(String, String)>, String> {
        let mut envs = Vec::new();
        let Some(deps) = &manifest.dependencies else {
            return Ok(envs);
        };
        let dc = crate::dependency_cache::DependencyCache::new();
        let mut unavailable: Vec<String> = Vec::new();
        for name in deps.keys() {
            let key = format!("PLUGKIT_{}", name.to_uppercase().replace('-', "_"));
            // 系统 PATH 已可用(如 ffmpeg)→ 直接复用,免下载
            if crate::dep_manifest::dep_available_on_path(name) {
                envs.push((key, name.to_string()));
                continue;
            }
            // 走共享缓存(下载或复用缓存)
            match dc.ensure_dependency(name, plugin_id).await {
                Ok(path) => envs.push((key, path.to_string_lossy().to_string())),
                Err(e) => {
                    // pinned 版本下载失败(如 GitHub 被墙)→ 回退到缓存中任意已存在版本,
                    // 保证依赖仍可用;日志给出真实原因而非静默。
                    if let Some(cached) = dc.find_cached_any_version(
                        name,
                        &crate::dep_manifest::current_platform(),
                    ) {
                        log::warn!(
                            "resolve dep {} for {}: {} (fallback to cached {})",
                            name, plugin_id, e, cached.display()
                        );
                        envs.push((key, cached.to_string_lossy().to_string()));
                    } else {
                        // 彻底不可用(无 PATH、无缓存、下载失败):收集,稍后任务直接报失败,
                        // 而非静默进入插件再报笼统的 YTDLP_MISSING。
                        log::error!("dependency {} for {} unavailable: {}", name, plugin_id, e);
                        unavailable.push(name.clone());
                    }
                }
            }
        }
        if !unavailable.is_empty() {
            return Err(format!(
                "共享依赖 {} 不可用：下载失败（镜像均不可达），请检查网络后重试",
                unavailable.join("、")
            ));
        }
        Ok(envs)
    }

    /// 定位插件可执行文件。优先用 manifest 的 entry.executable 精确定位;
    /// 找不到时回退到 tool_dir 下第一个"看起来可执行"的文件。
    /// (修复:原先盲扫第一个非 exe/dll 文件会把 index.html 当可执行文件)
    fn find_executable(tool_dir: &PathBuf, manifest: &crate::manifest_model::Manifest) -> Result<PathBuf, String> {
        // 1. 优先按 manifest 指定名
        let declared = tool_dir.join(&manifest.entry.executable);
        if declared.exists() && declared.is_file() {
            return Ok(declared);
        }

        // 2. 回退:找第一个可执行(跳过 html/js/css 等非可执行扩展名)
        for entry in std::fs::read_dir(tool_dir).map_err(|e| format!("Read tool dir: {}", e))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());
            // 跳过明显的非可执行文件
            if let Some(e) = &ext {
                if matches!(e.as_str(), "html" | "htm" | "js" | "css" | "json" | "txt" | "md" | "svg") {
                    continue;
                }
                if e == "exe" || e == "dll" { continue; }
            }
            // 无扩展名(macOS/linux 可执行惯例)优先
            if ext.is_none() && !path.to_string_lossy().contains('.') {
                return Ok(path);
            }
            return Ok(path);
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
                // 未提供的参数占位符(如 {output_dir} 为空)替换为空,避免字面量残留
                // 例: --output-dir {output_dir} → --output-dir ""(插件端回退默认路径)
                result = strip_unfilled_placeholders(&result);
                result
            })
            .collect()
    }

    /// Read stdout line by line, emitting progress events and capturing result/error lines.
    async fn read_stdout_protocol(        reader: impl AsyncRead + Unpin + Send + 'static,
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
                                // 同步任务中心:同一 task_id,任务中心可见真实进度。
                                crate::task_queue::TaskQueue::update_progress(
                                    &task_id,
                                    p.percent.unwrap_or(0.0),
                                    p.speed.clone(),
                                    p.eta.clone(),
                                    p.message.clone(),
                                    p.file_name.clone(),
                                    p.output_path.clone(),
                                ).await;
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

/// 把字符串里残留的 `{name}` 占位符替换为空(未提供的参数)。
///
/// ⚠️ 必须按 UTF-8 字符边界处理,不能逐字节重建字符串:
/// 旧实现用 `bytes[i] as char` 逐字节转码,会把中文等多字节字符拆坏
/// (曾导致中文路径变成乱码,插件报"输入文件不存在")。
fn strip_unfilled_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                // 剩余部分原样保留(含多字节字符)
                out.push_str(rest);
                break;
            }
            Some(idx) => {
                // '{' 之前的内容原样保留
                out.push_str(&rest[..idx]);
                let tail = &rest[idx..];
                // 尝试匹配 {identifier}
                if let Some(close_rel) = tail[1..].find('}') {
                    let name = &tail[1..1 + close_rel];
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        // 是占位符 → 整个跳过
                        rest = &tail[1 + close_rel + 1..];
                        continue;
                    }
                }
                // 不是占位符(如普通文本里的花括号)→ 保留这个 '{' 并继续
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_keeps_utf8_chinese_path_intact() {
        // 回归:中文等多字节字符不能被逐字节拆坏(曾导致"输入文件不存在")
        let path = "/Users/main/Documents/备用文件/fc2-ppv-1637514-HD/fc2-ppv-1637514-nyap2p.com.mp4";
        assert_eq!(strip_unfilled_placeholders(path), path);
    }

    #[test]
    fn strip_removes_placeholder_only() {
        assert_eq!(strip_unfilled_placeholders("--output-dir {output_dir}"), "--output-dir ");
        assert_eq!(strip_unfilled_placeholders("{output_dir}"), "");
        assert_eq!(strip_unfilled_placeholders("--quality {quality} --input {input}"), "--quality  --input ");
    }

    #[test]
    fn strip_keeps_non_placeholder_braces_and_mixed() {
        // 含空格/连字符的 {…} 不是 identifier 占位符 → 保留
        assert_eq!(strip_unfilled_placeholders("a{b c}d"), "a{b c}d");
        assert_eq!(strip_unfilled_placeholders("a{x-y}d"), "a{x-y}d");
        // 中文 + 占位符混合(中文是 alphanumeric,算占位符)
        assert_eq!(
            strip_unfilled_placeholders("下载到{dir}/备用文件夹"),
            "下载到/备用文件夹"
        );
    }
}
