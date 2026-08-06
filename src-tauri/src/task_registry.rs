//! 全局运行任务注册表:task_id → 取消标志。
//! ToolRunner 启动子进程时注册;task_cancel 时置位;进程结束后注销。
//! 使任务中心的「取消」真正杀掉子进程(修复原先只改状态不杀进程的缺陷)。

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

static RUNNING: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 注册一个运行中的任务,返回其取消标志。
pub async fn register(task_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    RUNNING.lock().await.insert(task_id.to_string(), flag.clone());
    flag
}

/// 请求取消任务(置位标志)。返回是否找到该任务。
pub async fn request_cancel(task_id: &str) -> bool {
    let guard = RUNNING.lock().await;
    if let Some(flag) = guard.get(task_id) {
        flag.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// 任务结束后注销。
pub async fn unregister(task_id: &str) {
    RUNNING.lock().await.remove(task_id);
}
