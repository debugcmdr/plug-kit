use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;

static TASK_QUEUE: Lazy<Mutex<TaskQueue>> = Lazy::new(|| Mutex::new(TaskQueue::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub plugin_id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub progress: TaskProgress,
    /// 失败原因(任务中心展示;成功/运行中为 None)。
    #[serde(default)]
    pub error: Option<String>,
    /// 转换文件名(插件 progress 上报,任务中心展示)。
    #[serde(default)]
    pub file_name: Option<String>,
    /// 输出文件/目录路径(任务中心"打开输出文件夹"按钮使用)。
    #[serde(default)]
    pub output_path: Option<String>,
    /// 任务来源链接(提取类任务记录,用于去重/排查)。
    #[serde(default)]
    pub url: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub percent: f64,
    pub speed: Option<String>,
    pub eta: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            speed: None,
            eta: None,
            message: None,
        }
    }
}

#[derive(Debug)]
pub struct TaskQueue {
    tasks: HashMap<String, Task>,
}

impl TaskQueue {
    /// 启动时从磁盘恢复任务历史(修复"写而不读":tasks.json 落盘的历史任务
    /// 跨会话可见,recover() 也才能标记上次遗留的非终态任务)。
    pub fn new() -> Self {
        Self {
            tasks: Self::load_from_disk(),
        }
    }

    /// 从 ~/.plugkit/tasks/tasks.json 恢复历史任务。
    /// 文件缺失/损坏时回退空表(损坏不阻塞启动,下次 save 原子覆盖)。
    fn load_from_disk() -> HashMap<String, Task> {
        let tasks_file = Self::tasks_path();
        if !tasks_file.exists() {
            return HashMap::new();
        }
        match fs::read_to_string(&tasks_file) {
            Ok(content) => serde_json::from_str::<Vec<Task>>(&content)
                .map(|v| v.into_iter().map(|t| (t.task_id.clone(), t)).collect())
                .unwrap_or_default(),
            Err(_) => HashMap::new(),
        }
    }

    pub async fn list() -> Vec<serde_json::Value> {
        let queue = TASK_QUEUE.lock().await;
        let mut tasks: Vec<&Task> = queue.tasks.values().collect();
        // 按创建时间降序(最新在前):HashMap 本身无序,任务中心历史需稳定倒序展示。
        // created_at 为同一格式(UTC rfc3339)的字符串,字典序即时间序。
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks.into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect()
    }

    pub async fn create(plugin_id: &str, type_: &str, url: Option<String>) -> String {
        let mut queue = TASK_QUEUE.lock().await;
        Self::create_inner(&mut queue, plugin_id, type_, url)
    }

    /// 原子「查重 + 创建」:在锁内检查同插件+同 URL 的活跃任务。
    /// 修复 TOCTOU 竞态——原先 find_duplicate(检查)与 create(创建)各自
    /// 取锁、非原子,并发双击提取同一链接会穿透检查创建双任务。
    /// 命中重复返回 None(调用方提示),否则创建并返回新 task_id。
    /// 注意:必须内联检查(不能复用 find_duplicate),本方法持有锁,再取会死锁。
    pub async fn create_with_dedup(plugin_id: &str, type_: &str, url: Option<String>) -> Option<String> {
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(url) = &url {
            let dup = queue.tasks.values().any(|t| {
                t.plugin_id == plugin_id
                    && t.url.as_deref() == Some(url)
                    && matches!(t.status, TaskStatus::Pending | TaskStatus::Running | TaskStatus::Paused)
            });
            if dup {
                return None;
            }
        }
        Some(Self::create_inner(&mut queue, plugin_id, type_, url))
    }

    /// 锁内创建任务(调用方须已持有 TASK_QUEUE 锁)。
    fn create_inner(queue: &mut TaskQueue, plugin_id: &str, type_: &str, url: Option<String>) -> String {
        let task_id = uuid::Uuid::new_v4().to_string();
        let task = Task {
            task_id: task_id.clone(),
            plugin_id: plugin_id.to_string(),
            type_: type_.to_string(),
            status: TaskStatus::Pending,
            progress: TaskProgress::default(),
            error: None,
            file_name: None,
            output_path: None,
            url,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
        };
        queue.tasks.insert(task_id.clone(), task);
        queue.save().unwrap_or(());
        task_id
    }

    pub async fn update_status(task_id: &str, status: TaskStatus) {
        Self::update_status_with_error(task_id, status, None).await;
    }

    /// 更新任务状态,并可选记录失败原因(任务中心直接展示)。
    pub async fn update_status_with_error(task_id: &str, status: TaskStatus, error: Option<String>) {
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            task.status = status.clone();
            if let Some(e) = error {
                task.error = Some(e);
            } else if matches!(status, TaskStatus::Running | TaskStatus::Pending) {
                task.error = None; // 重新开始执行时清掉历史错误
            }
            if matches!(status, TaskStatus::Running) {
                task.started_at = Some(chrono::Utc::now().to_rfc3339());
            } else if matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Interrupted) {
                task.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            queue.save().unwrap_or(());
        }
    }

    pub async fn update_progress(
        task_id: &str,
        percent: f64,
        speed: Option<String>,
        eta: Option<String>,
        message: Option<String>,
        file_name: Option<String>,
        output_path: Option<String>,
    ) {
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            task.progress = TaskProgress { percent, speed, eta, message };
            // 插件 progress 行可能只携带部分字段,缺省时保留任务已有值。
            if file_name.is_some() {
                task.file_name = file_name;
            }
            if output_path.is_some() {
                task.output_path = output_path;
            }
            // 进度是易失数据(仅内存),不落盘:任务中心 UI 走内存 + 3s 轮询,
            // 崩溃恢复由 recover() 依据落盘的状态字段处理——高频全量写 tasks.json
            // 只会浪费磁盘 IO,无人读取。仅在状态迁移(update_status_*)时落盘。
        }
    }

    /// 应用启动时调用：把上次运行遗留的非终态任务（pending/running/paused）
    /// 标记为 interrupted —— 子进程随主程序退出已消失，不能再假装它们还在跑。
    pub async fn recover() {
        let mut queue = TASK_QUEUE.lock().await;
        let mut changed = false;
        for task in queue.tasks.values_mut() {
            if matches!(
                task.status,
                TaskStatus::Pending | TaskStatus::Running | TaskStatus::Paused
            ) {
                task.status = TaskStatus::Interrupted;
                task.error = Some("应用重启,任务已中断".into());
                task.completed_at = Some(chrono::Utc::now().to_rfc3339());
                changed = true;
            }
        }
        if changed {
            queue.save().unwrap_or(());
        }
    }

    /// 查询任务归属插件(不存在返回 None)。供 bridge 层校验 task_id 归属(G-4)。
    pub async fn owner_of(task_id: &str) -> Option<String> {
        let queue = TASK_QUEUE.lock().await;
        queue.tasks.get(task_id).map(|t| t.plugin_id.clone())
    }

    /// Cancel a task: mark Cancelled AND request the running subprocess be killed
    /// via the global task registry (fixes cancel that only changed status).
    /// 仅活跃任务(pending/running/paused)可取消;终态任务忽略,避免非法状态迁移。
    pub async fn cancel(task_id: &str) {
        // 通知 ToolRunner 杀子进程(若该任务正在运行)
        let _ = crate::task_registry::request_cancel(task_id).await;

        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            if matches!(task.status, TaskStatus::Pending | TaskStatus::Running | TaskStatus::Paused) {
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(chrono::Utc::now().to_rfc3339());
                queue.save().unwrap_or(());
            }
        }
    }

    /// Pause a task: 请求暂停子进程(SIGSTOP/挂起)+ 标记状态 Paused。
    /// 真实暂停,不是只改状态——ToolRunner 收到 pause 标志后暂停整个任务树。
    /// 仅 pending/running 可暂停(已暂停幂等跳过)。
    pub async fn pause(task_id: &str) {
        // 通知 ToolRunner 暂停子进程(若该任务正在运行/排队)
        let _ = crate::task_registry::request_pause(task_id).await;
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            if matches!(task.status, TaskStatus::Pending | TaskStatus::Running) {
                task.status = TaskStatus::Paused;
                queue.save().unwrap_or(());
            }
        }
    }

    /// Resume a paused task: 请求恢复子进程(SIGCONT)+ 标记状态 Running。
    /// 仅 paused 可恢复。
    pub async fn resume(task_id: &str) {
        let _ = crate::task_registry::request_resume(task_id).await;
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            if task.status == TaskStatus::Paused {
                task.status = TaskStatus::Running;
                task.completed_at = None;
                if task.started_at.is_none() {
                    task.started_at = Some(chrono::Utc::now().to_rfc3339());
                }
                queue.save().unwrap_or(());
            }
        }
    }

    /// 任务历史保留期(天):终态任务超期自动清理,防止 tasks.json 无限增长。
    const RETENTION_DAYS: i64 = 7;

    fn save(&mut self) -> Result<(), String> {
        // 自动清理:终态(completed/failed/cancelled/interrupted)且超保留期(7 天)的任务。
        let cutoff_ts = (chrono::Utc::now() - chrono::Duration::days(Self::RETENTION_DAYS)).timestamp();
        self.tasks.retain(|_, t| {
            if !matches!(
                t.status,
                TaskStatus::Completed
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
                    | TaskStatus::Interrupted
            ) {
                return true; // 非终态(排队/运行/暂停)始终保留
            }
            // 以完成时间(无则创建时间)判断是否超期
            let end_ts = t.completed_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .or_else(|| chrono::DateTime::parse_from_rfc3339(&t.created_at).ok())
                .map(|d| d.timestamp())
                .unwrap_or(0);
            end_ts > cutoff_ts
        });

        let tasks_file = Self::tasks_path();
        if let Some(parent) = tasks_file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&self.tasks.values().collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        let tmp = tasks_file.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &tasks_file).map_err(|e| e.to_string())
    }

    fn tasks_path() -> PathBuf {
        dirs::home_dir()
            .map(|d| d.join(".plugkit/tasks/tasks.json"))
            .unwrap_or_default()
    }
}
