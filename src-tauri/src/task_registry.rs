//! 全局运行任务注册表:task_id → 控制标志(cancel + pause)。
//! ToolRunner 启动子进程前注册;task_cancel / task_pause / task_resume 置位;
//! 进程结束后注销。使任务中心的「取消/暂停/恢复」真正作用于子进程。

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 单个任务的运行控制标志。
#[derive(Debug, Default)]
pub struct TaskControl {
    /// 取消请求:置位后 ToolRunner 杀子进程。
    pub cancel: Arc<AtomicBool>,
    /// 暂停请求:置位后 ToolRunner 对子进程(及进程组)发 SIGSTOP。
    pub pause: Arc<AtomicBool>,
}

impl TaskControl {
    pub fn new() -> Self {
        Self::default()
    }
}

static RUNNING: Lazy<Mutex<HashMap<String, Arc<TaskControl>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 注册一个运行(或排队)中的任务,返回其控制标志。
/// 注意:在并发排队之前就注册,使排队中的任务也能被取消/暂停。
pub async fn register(task_id: &str) -> Arc<TaskControl> {
    let control = Arc::new(TaskControl::new());
    RUNNING.lock().await.insert(task_id.to_string(), control.clone());
    control
}

/// 请求取消任务(置位 cancel 标志)。返回是否找到该任务。
pub async fn request_cancel(task_id: &str) -> bool {
    let guard = RUNNING.lock().await;
    if let Some(ctl) = guard.get(task_id) {
        ctl.cancel.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// 请求暂停任务(置位 pause 标志)。返回是否找到该任务。
pub async fn request_pause(task_id: &str) -> bool {
    let guard = RUNNING.lock().await;
    if let Some(ctl) = guard.get(task_id) {
        ctl.pause.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// 请求恢复任务(清除 pause 标志)。返回是否找到该任务。
pub async fn request_resume(task_id: &str) -> bool {
    let guard = RUNNING.lock().await;
    if let Some(ctl) = guard.get(task_id) {
        ctl.pause.store(false, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// 任务结束后注销。
pub async fn unregister(task_id: &str) {
    RUNNING.lock().await.remove(task_id);
}
