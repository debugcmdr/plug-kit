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
    pub type_: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub progress: TaskProgress,
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
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            speed: None,
            eta: None,
        }
    }
}

#[derive(Debug)]
pub struct TaskQueue {
    tasks: HashMap<String, Task>,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    pub async fn list() -> Vec<serde_json::Value> {
        let queue = TASK_QUEUE.lock().await;
        queue.tasks.values()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect()
    }

    pub async fn create(plugin_id: &str, type_: &str) -> String {
        let mut queue = TASK_QUEUE.lock().await;
        let task_id = uuid::Uuid::new_v4().to_string();
        let task = Task {
            task_id: task_id.clone(),
            plugin_id: plugin_id.to_string(),
            type_: type_.to_string(),
            status: TaskStatus::Pending,
            progress: TaskProgress::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
        };
        queue.tasks.insert(task_id.clone(), task);
        queue.save().unwrap_or(());
        task_id
    }

    pub async fn update_status(task_id: &str, status: TaskStatus) {
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            task.status = status.clone();
            if matches!(status, TaskStatus::Running) {
                task.started_at = Some(chrono::Utc::now().to_rfc3339());
            } else if matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled) {
                task.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            queue.save().unwrap_or(());
        }
    }

    pub async fn update_progress(task_id: &str, percent: f64, speed: Option<String>, eta: Option<String>) {
        let mut queue = TASK_QUEUE.lock().await;
        if let Some(task) = queue.tasks.get_mut(task_id) {
            task.progress = TaskProgress { percent, speed, eta };
            queue.save().unwrap_or(());
        }
    }

    fn save(&self) -> Result<(), String> {
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
