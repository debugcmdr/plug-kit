use std::fs;
use std::io::Write;
use chrono::Local;
use once_cell::sync::Lazy;

/// 日志保留天数:超过该天数的 .log 文件在写入时自动清理。
const LOG_RETENTION_DAYS: i64 = 14;
/// 日志清理最小间隔:避免每次写日志都全量 read_dir 扫描目录(高频 IO)。
const CLEANUP_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
static LAST_CLEANUP: Lazy<std::sync::Mutex<Option<std::time::Instant>>> =
    Lazy::new(|| std::sync::Mutex::new(None));

pub struct Logger;

impl Logger {
    pub fn info(msg: &str) {
        Self::write_log("INFO", msg);
    }

    pub fn error(msg: &str) {
        Self::write_log("ERROR", msg);
    }

    /// 写入 WARN 级别日志(预留,供 bridge/log_warn 使用)。
    #[allow(dead_code)]
    pub fn warn(msg: &str) {
        Self::write_log("WARN", msg);
    }

    /// 任务错误:结构化 JSON 行(日志页渲染为可展开条目)。
    /// 比插件 UI 的错误更完整:含错误码 + 完整 message(插件把 details 拼进 message)。
    pub fn task_error(plugin_id: &str, task_id: &str, code: &str, message: &str) {
        let msg = format!(
            "{{\"plugin\":{},\"task_id\":{},\"code\":{},\"message\":{}}}",
            serde_json::to_string(plugin_id).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(task_id).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(code).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into()),
        );
        Self::write_log("TASK_ERROR", &msg);
    }

    fn write_log(level: &str, msg: &str) {
        let log_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".plugkit")
            .join("logs");
        let log_file = log_dir.join(format!("{}.log", Local::now().format("%Y-%m-%d")));

        if let Some(parent) = log_file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // 轮转:清理超过保留天数的旧日志文件(按文件名日期解析)
        Self::cleanup_old_logs(&log_dir);

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{}] [{}] {}\n", timestamp, level, msg);

        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn cleanup_old_logs(log_dir: &std::path::Path) {
        // 降频:60s 内只清理一次(原实现每次写日志都全量扫描目录)
        {
            let mut last = LAST_CLEANUP.lock().unwrap();
            if last.map_or(false, |t| t.elapsed() < CLEANUP_MIN_INTERVAL) {
                return;
            }
            *last = Some(std::time::Instant::now());
        }
        let Ok(entries) = fs::read_dir(log_dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !name.ends_with(".log") {
                continue;
            }
            // 文件名形如 2026-08-21.log
            let date_part = name.trim_end_matches(".log");
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                let age_days = (chrono::Local::now().date_naive() - date).num_days();
                if age_days > LOG_RETENTION_DAYS {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}
