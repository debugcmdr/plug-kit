use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
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

/// 杀进程树(取消/超时):Unix 对进程组发 SIGTERM → 短暂等待 → SIGKILL 兜底,
/// Windows 用 taskkill /T /F(含子进程)。仅 kill 主进程会让 yt-dlp 拉起的
/// ffmpeg 残留为孤儿继续写盘——进程组信号可一并终止。
async fn kill_process_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        {
            // 负数 pid = 进程组信号(子进程放入了独立进程组 process_group(0))
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &format!("-{}", pid)])
                .status();
            // 给组内成员 200ms 优雅退出,再补 SIGKILL 防残留
            // (用异步 sleep 而非 thread::sleep:后者会阻塞 tokio worker 线程,R-5)
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &format!("-{}", pid)])
                .status();
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .status();
        }
    }
    let _ = child.kill().await; // 主进程兜底(SIGKILL)
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
            // 任务工作目录与插件安装临时目录(~/.plugkit/tmp)分离：
            // 安装插件时清理 tmp/{plugin_id} 不会误删运行中任务的工作目录。
            work_dir: crate::paths::work_dir(),
        }
    }

    /// 前置校验失败统一处理：注销控制标志 + 任务状态置 Failed。
    /// (修复状态机闭环缺口：此前 invoke 前置校验 `?` 提前返回，
    /// 任务中心的任务永久停留在 Pending。)
    async fn fail_task(task_id: &str, msg: &str) {
        crate::task_registry::unregister(task_id).await;
        crate::task_queue::TaskQueue::update_status_with_error(
            task_id, crate::task_queue::TaskStatus::Failed, Some(msg.to_string()),
        ).await;
        log::error!("[{}] {}", task_id, msg);
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
        // 任务 ID：bridge 层创建后透传；独立调用(测试/无 UI 场景)则内部生成。
        let task_id = task_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // 前置校验(错误前置)：manifest/命令/exe 任一缺失即任务失败。
        // 状态同步更新为 Failed——否则任务中心永久停留 Pending(状态机不闭环)。
        let plugin_dir = self.plugin_dir(plugin_id);
        let manifest = match self.read_manifest(plugin_id) {
            Ok(m) => m,
            Err(e) => {
                Self::fail_task(&task_id, &e).await;
                return Err(e);
            }
        };
        let command = match manifest.commands.get(command_name) {
            Some(c) => c,
            None => {
                let msg = format!("Command '{}' not found in manifest", command_name);
                Self::fail_task(&task_id, &msg).await;
                return Err(msg);
            }
        };
        let exe = match Self::find_executable(&plugin_dir.join(manifest.tool_dir()), &manifest) {
            Ok(e) => e,
            Err(msg) => {
                Self::fail_task(&task_id, &msg).await;
                return Err(msg);
            }
        };
        let args = self.substitute_args(&command.stdin_args, &params, &manifest);

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
        // 此阶段在解析计时之外——插件 info() 的超时只计解析时长,不含依赖下载。
        // 文案按 manifest 声明的依赖动态拼接(外壳不硬编码具体依赖名——零业务逻辑)。
        let dep_names = manifest.dependencies.as_ref()
            .map(|d| d.keys().cloned().collect::<Vec<_>>().join("、"))
            .unwrap_or_default();
        crate::task_queue::TaskQueue::update_progress(
            &task_id, 0.0, None, None,
            Some(if dep_names.is_empty() {
                "正在准备运行环境…".to_string()
            } else {
                format!("正在准备共享依赖（{}）…", dep_names)
            }),
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
                crate::logger::Logger::task_error(plugin_id, &task_id, "DEP_RESOLVE_FAILED", &e);
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
        // json 模式命令(list-formats/probe 等短命令)带 5 分钟超时防挂死；
        // progress 模式(长下载/转换)不设超时。
        let is_progress = command.output_mode == "progress+json";
        let timeout = if is_progress {
            None
        } else {
            Some(std::time::Duration::from_secs(300))
        };
        let outcome = Self::await_with_cancel(&mut child, &cancel_flag, &pause_flag, is_progress, timeout).await;
        if let Some(flag) = outcome.killed {
            // Cancelled / timed out：杀整个进程树(进程组 SIGTERM→SIGKILL)，
            // 避免只杀主进程留下孤儿 ffmpeg(yt-dlp 拉起的)继续写盘。
            kill_process_tree(&mut child).await;
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
            crate::logger::Logger::task_error(&plugin_id, &task_id, "EXIT_FAILURE", &msg);
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
                // 错误码随 message 一并上抛(G-1):任务中心/插件 UI 此前只能拿到
                // message 被迫字符串匹配;带 [CODE] 前缀后三级链路均可按码识别。
                // (message 原文保留,插件 UI 对 "完全磁盘访问" 等内容匹配不受影响)
                let prefixed = format!("[{}] {}", e.code, e.message);
                crate::task_queue::TaskQueue::update_status_with_error(
                    &task_id,
                    crate::task_queue::TaskStatus::Failed,
                    Some(prefixed.clone()),
                ).await;
                log::error!("[{}] plugin error: {}", task_id, e.message);
                // 落盘日志页:结构化任务错误(错误码 + 完整 message,比插件 UI 更详细)。
                crate::logger::Logger::task_error(&plugin_id, &task_id, &e.code, &e.message);
            }
            Ok(PluginOutput::Result(r)) => {
                if r.success {
                    crate::task_queue::TaskQueue::update_status(&task_id, crate::task_queue::TaskStatus::Completed).await;
                } else {
                    // 协议允许 success=false 的 result 行:按失败处理,不能标记 Completed。
                    let msg = "插件返回 success=false".to_string();
                    crate::task_queue::TaskQueue::update_status_with_error(
                        &task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone()),
                    ).await;
                    log::error!("[{}] {}", task_id, msg);
                    crate::logger::Logger::task_error(&plugin_id, &task_id, "RESULT_FAILED", &msg);
                }
            }
            _ => {
                // 协议违规:最后一行不是 result/error(如 progress 行或不可解析输出)。
                // 静默吞掉会让插件 UI 拿到错误形状的数据——明确报错并落日志。
                let msg = if final_outcome.is_ok() {
                    "插件输出协议违规:最后一行应为 result 或 error".to_string()
                } else {
                    format!("插件输出无法解析: {}", last)
                };
                crate::task_queue::TaskQueue::update_status_with_error(
                    &task_id, crate::task_queue::TaskStatus::Failed, Some(msg.clone()),
                ).await;
                log::error!("[{}] {}", task_id, msg);
                crate::logger::Logger::task_error(&plugin_id, &task_id, "BAD_PROTOCOL", &msg);
            }
        }
        let _ = std::fs::remove_dir_all(&work_dir);

        match final_outcome {
            Ok(PluginOutput::Result(r)) => Ok(r.data),
            Ok(PluginOutput::Error(e)) => Err(format!("[{}] {}", e.code, e.message)),
            _ => Err("插件输出协议违规:最后一行应为 result 或 error".to_string()),
        }
    }

    /// Wait for the child to exit, watching cancel / pause flags.
    /// 取消(cancel)恒检查:json 模式命令(list-formats 等)也可被任务中心取消,
    /// 否则网络卡顿时任务永久 running 直到子进程自然退出。
    /// 暂停(pause)仅 progress 模式开启(watch_pause)——短命令暂停无意义。
    /// `timeout`:Some 时超时后按 killed 返回(json 模式短命令防挂死)。
    async fn await_with_cancel(
        child: &mut Child,
        cancel_flag: &Arc<AtomicBool>,
        pause_flag: &Arc<AtomicBool>,
        watch_pause: bool,
        timeout: Option<std::time::Duration>,
    ) -> ProcessOutcome {
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);
        loop {
            // 超时(仅 json 模式):kill 由调用方执行,这里返回 killed 标记。
            if let Some(deadline) = deadline {
                if tokio::time::Instant::now() >= deadline {
                    return ProcessOutcome { success: false, code: -1, killed: Some("Task timed out".into()) };
                }
            }
            // If cancellation was requested, stop waiting (始终检查).
            if cancel_flag.load(Ordering::SeqCst) {
                return ProcessOutcome { success: false, code: -1, killed: Some("Task cancelled".into()) };
            }
            // Pause requested: suspend the process (and its group), then wait for resume.
            if watch_pause && pause_flag.load(Ordering::SeqCst) {
                // 仅当进程还存活才暂停:若任务在暂停请求到达前已结束,对死进程
                // SIGSTOP 后等 resume 会永久卡死(任务永不进入终态,须用户手动恢复)。
                let alive = matches!(child.try_wait(), Ok(None));
                if alive {
                    suspend_process(child);
                    while pause_flag.load(Ordering::SeqCst) {
                        if cancel_flag.load(Ordering::SeqCst) {
                            // 先恢复再取消:SIGKILL 对被 STOP 的进程无效
                            resume_process(child);
                            return ProcessOutcome { success: false, code: -1, killed: Some("Task cancelled".into()) };
                        }
                        // 暂停期间进程自行退出(崩溃/被外部终止)→ 不再等 resume,交由下方 try_wait 收尾
                        if matches!(child.try_wait(), Ok(Some(_))) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    resume_process(child);
                }
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
        let platform = crate::dep_manifest::current_platform();
        let mut unavailable: Vec<String> = Vec::new();
        // 收集「来自共享缓存的依赖所在目录」,循环后前置注入 PATH——yt-dlp 等子进程
        // (及其派生的 ffmpeg/ffprobe)靠 PATH 查找可执行文件,仅注入 PLUGKIT_* 环境变量
        // 并不够(yt-dlp 不读它)。系统 PATH 已可用的依赖(裸名)无需收集。
        let mut dep_dirs: Vec<std::path::PathBuf> = Vec::new();
        for name in deps.keys() {
            let key = format!("PLUGKIT_{}", name.to_uppercase().replace('-', "_"));
            // 系统 PATH 已可用(如 ffmpeg)→ 直接复用,免下载
            if crate::dep_manifest::dep_available_on_path(name) {
                envs.push((key, name.to_string()));
                continue;
            }
            // 运行时只查共享缓存(不递增引用计数——引用关系在「安装」时已登记,
            // 卸载按 refs 扫描递减,两侧严格对称;若此处再递增会导致孤儿依赖残留)。
            if let Some(cached) = dc.find_cached_any_version(name, &platform) {
                if let Some(dir) = cached.parent() {
                    if !dep_dirs.iter().any(|d| d == dir) {
                        dep_dirs.push(dir.to_path_buf());
                    }
                }
                envs.push((key, cached.to_string_lossy().to_string()));
                continue;
            }
            // 缓存完全不存在(如安装后缓存被手动删除)→ 兜底重新安装(下载 + 登记引用)。
            // 此时登记的引用会在卸载时被 decrement_refcount_for_plugin 正确回收。
            match dc.ensure_dependency(name, plugin_id).await {
                Ok(path) => {
                    if let Some(dir) = path.parent() {
                        if !dep_dirs.iter().any(|d| d == dir) {
                            dep_dirs.push(dir.to_path_buf());
                        }
                    }
                    envs.push((key, path.to_string_lossy().to_string()))
                }
                Err(e) => {
                    // 彻底不可用(无 PATH、无缓存、下载失败):收集,稍后任务直接报失败,
                    // 而非静默进入插件再报笼统的 YTDLP_MISSING。
                    log::error!("dependency {} for {} unavailable: {}", name, plugin_id, e);
                    unavailable.push(name.clone());
                }
            }
        }
        // 依赖目录前置注入 PATH(子进程继承):yt-dlp 合并/提取时内部 spawn 的
        // ffmpeg/ffprobe 据此找到可执行文件。共享缓存目录在 PATH 最前,不覆盖
        // 用户原有 PATH。
        if !dep_dirs.is_empty() {
            let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
            let mut parts: Vec<String> = dep_dirs
                .iter()
                .map(|d| d.to_string_lossy().to_string())
                .collect();
            if let Ok(cur) = std::env::var("PATH") {
                parts.push(cur);
            }
            envs.push(("PATH".to_string(), parts.join(sep)));
        }
        if !unavailable.is_empty() {
            return Err(format!(
                "共享依赖 {} 不可用：下载失败（镜像均不可达），请检查网络后重试",
                unavailable.join("、")
            ));
        }
        Ok(envs)
    }

    /// 定位插件可执行文件:按 manifest 的 entry.executable 精确定位。
    /// (安装阶段已校验该文件存在——install 对 manifest.json 与 executable 做
    /// 错误前置,此处不再盲扫 tool_dir,避免把 index.html 之类误当可执行文件。)
    fn find_executable(tool_dir: &PathBuf, manifest: &crate::manifest_model::Manifest) -> Result<PathBuf, String> {
        let declared = tool_dir.join(&manifest.entry.executable);
        if declared.exists() && declared.is_file() {
            return Ok(declared);
        }
        Err(format!(
            "Executable '{}' not found in tool dir (plugin 未通过安装校验或文件被篡改)",
            manifest.entry.executable
        ))
    }

    fn read_manifest(&self, plugin_id: &str) -> Result<crate::manifest_model::Manifest, String> {
        let manifest_path = self.plugin_dir(plugin_id).join("manifest.json");
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse manifest: {}", e))
    }

    fn plugin_dir(&self, plugin_id: &str) -> PathBuf {
        crate::paths::plugins_dir().join(plugin_id)
    }

    /// Substitute `{var}` placeholders from params, plus `{platform}`.
    fn substitute_args(&self, args: &[String], params: &Value, _manifest: &crate::manifest_model::Manifest) -> Vec<String> {
        // 平台串统一由 dep_manifest::current_platform() 提供(G-7:避免第三处
        // 内联重复判断——plugin_manager/tool_runner 曾各写一份)。
        let platform = crate::dep_manifest::current_platform();

        // 模板中出现的占位符集合(替换前):strip 阶段只清理"模板存在但参数未提供"的
        // 占位符,绝不能对任意 {identifier} 下手——用户文件路径里的 {1} 等花括号
        // 文本会被误删(R-7)。
        let template_keys: std::collections::HashSet<String> = args
            .iter()
            .flat_map(|a| extract_template_placeholders(a))
            .collect();
        let provided: std::collections::HashSet<String> = match params {
            Value::Object(map) => map.keys().cloned().collect(),
            _ => std::collections::HashSet::new(),
        };
        let unfilled: std::collections::HashSet<String> =
            template_keys.difference(&provided).cloned().collect();

        // 一次性替换(B-8):单次遍历 arg,遇到 {name} 直接查值替换,替换结果不再
        // 参与后续替换——原实现按 params 逐 key replace,某个参数值里含的字面
        // {other_key}(如用户文件名 "报告{output}版.mp4")会被下一轮错误替换污染。
        args.iter()
            .map(|arg| substitute_arg_once(arg, &platform, params, &unfilled))
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
        // 只保留末尾 MAX_KEPT_LINES 行(R-6):最终只用最后一行(result/error),
        // 长任务(数小时下载)会输出数千条 progress,全量累积纯属内存浪费。
        const MAX_KEPT_LINES: usize = 100;
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
                                // percent clamp 到 0-100(插件为不可信代码,可能上报
                                // 负数/超 100 的异常值,前端进度条按百分比渲染)。
                                let clamped = p.percent.map(|v| v.clamp(0.0, 100.0));
                                crate::task_queue::TaskQueue::update_progress(
                                    &task_id,
                                    clamped.unwrap_or(0.0),
                                    p.speed.clone(),
                                    p.eta.clone(),
                                    p.message.clone(),
                                    p.file_name.clone(),
                                    p.output_path.clone(),
                                ).await;
                            }
                        }
                    }
                    if lines.len() >= MAX_KEPT_LINES {
                        lines.remove(0);
                    }
                    lines.push(trimmed);
                }
                Err(_) => break,
            }
        }
        lines
    }

    /// 读取并丢弃 stderr(R-6):stderr 数据从不被使用(诊断靠插件 stdout 的 error 行),
    /// 读取只是为了排空管道防止子进程写阻塞;全量 read_to_string 会白白累积内存。
    async fn read_stderr(
        reader: impl AsyncRead + Unpin + Send + 'static,
    ) -> Result<String, String> {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => return Err(format!("Failed to read stderr: {}", e)),
            }
        }
        Ok(String::new())
    }
}

struct ProcessOutcome {
    success: bool,
    code: i32,
    killed: Option<String>,
}

/// 提取字符串中的占位符名集合(`{name}`,name 为字母数字/下划线)。
/// 用于确定 manifest 模板中"声明了哪些占位符"。
fn extract_template_placeholders(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let tail = &rest[open..];
        if let Some(close_rel) = tail[1..].find('}') {
            let name = &tail[1..1 + close_rel];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                out.push(name.to_string());
                rest = &tail[1 + close_rel + 1..];
                continue;
            }
        }
        rest = &tail[1..];
    }
    out
}

/// 单参数一次性占位符替换(替代原"逐 key replace + 二次 strip"两段式):
/// - `{platform}` → 平台串(恒替换)
/// - `{name}`(params 含该键) → 参数值;替换值不再参与后续替换(B-8)
/// - `{name}`(模板声明过但本次未提供,即 unfilled) → 清空,避免字面量残留
///   例: --output-dir {output_dir} → --output-dir ""(插件端回退默认路径)
/// - 其余 `{...}`(用户文本/非标识符,如路径里的 {2}) → 原样保留
///
/// ⚠️ 必须按 UTF-8 字符边界处理,不能逐字节重建字符串:
/// 旧实现用 `bytes[i] as char` 逐字节转码,会把中文等多字节字符拆坏
/// (曾导致中文路径变成乱码,插件报"输入文件不存在")。
fn substitute_arg_once(
    arg: &str,
    platform: &str,
    params: &Value,
    unfilled: &std::collections::HashSet<String>,
) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        match tail[1..].find('}') {
            Some(close_rel) => {
                let name = &tail[1..1 + close_rel];
                let is_ident = !name.is_empty()
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_');
                let value = if name == "platform" {
                    Some(platform.to_string())
                } else if is_ident {
                    match params {
                        Value::Object(map) => map.get(name).map(|v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        }),
                        _ => None,
                    }
                    .or_else(|| {
                        if unfilled.contains(name) {
                            Some(String::new()) // 模板占位符未提供 → 清空
                        } else {
                            None
                        }
                    })
                } else {
                    None // 非标识符(如 {x-y})不算占位符 → 保留
                };
                match value {
                    Some(v) => {
                        out.push_str(&v);
                        rest = &tail[1 + close_rel + 1..];
                    }
                    None => {
                        // 非模板占位符(用户路径/普通文本里的花括号)→ 保留 '{'
                        out.push('{');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    fn sub(arg: &str, params: &Value, unfilled: &HashSet<String>) -> String {
        substitute_arg_once(arg, "macos-arm64", params, unfilled)
    }

    #[test]
    fn strip_keeps_utf8_chinese_path_intact() {
        // 回归:中文等多字节字符不能被逐字节拆坏(曾导致"输入文件不存在")
        let path = "/Users/main/Documents/备用文件/fc2-ppv-1637514-HD/fc2-ppv-1637514-nyap2p.com.mp4";
        assert_eq!(sub(path, &json!({}), &set(&[])), path);
    }

    #[test]
    fn strip_removes_placeholder_only() {
        assert_eq!(sub("--output-dir {output_dir}", &json!({}), &set(&["output_dir"])), "--output-dir ");
        assert_eq!(sub("{output_dir}", &json!({}), &set(&["output_dir"])), "");
        assert_eq!(
            sub("--quality {quality} --input {input}", &json!({}), &set(&["quality", "input"])),
            "--quality  --input "
        );
    }

    #[test]
    fn strip_keeps_non_placeholder_braces_and_mixed() {
        // 含空格/连字符的 {…} 不是 identifier 占位符 → 保留
        assert_eq!(sub("a{b c}d", &json!({}), &set(&[])), "a{b c}d");
        assert_eq!(sub("a{x-y}d", &json!({}), &set(&[])), "a{x-y}d");
        // 中文 + 占位符混合(中文是 alphanumeric,算占位符);{dir} 在 unfilled 中才删
        assert_eq!(
            sub("下载到{dir}/备用文件夹", &json!({}), &set(&["dir"])),
            "下载到/备用文件夹"
        );
    }

    #[test]
    fn strip_keeps_user_path_braces_not_in_template() {
        // R-7 回归:用户文件路径中的 {数字} 等花括号文本不属于模板占位符 → 必须保留
        let path = "/a/报告{2}最终版.mp4";
        assert_eq!(sub(path, &json!({}), &set(&["output"])), path);
        assert_eq!(sub(path, &json!({}), &set(&[])), path);
    }

    #[test]
    fn params_replace_once_no_second_pass_pollution() {
        // B-8 回归:参数值里的字面 {other_key} 不得被后续替换污染。
        // 原实现逐 key replace:input 值含 {output} → 被 output 值 "y" 二次污染。
        let params = json!({ "input": "x{output}.mp4", "output": "y" });
        assert_eq!(sub("{input}", &params, &set(&[])), "x{output}.mp4");
    }

    #[test]
    fn platform_placeholder_replaced() {
        assert_eq!(sub("--p {platform}", &json!({}), &set(&[])), "--p macos-arm64");
        // params 里的 platform 键不覆盖 {platform} 恒替换语义(与原实现一致)
        assert_eq!(sub("{platform}", &json!({ "platform": "x" }), &set(&[])), "macos-arm64");
    }

    #[test]
    fn extract_template_keys_collects_identifiers() {
        let keys = extract_template_placeholders("--input {input} --output {output_dir} {x-y} {1}");
        assert_eq!(keys, vec!["input", "output_dir", "1"]);
        // 非 identifier({x-y})不收集
    }
}
