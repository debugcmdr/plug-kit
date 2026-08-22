use std::fs;
use std::io::Write;
use chrono::Local;

/// 日志保留天数:超过该天数的 .log 文件在写入时自动清理。
const LOG_RETENTION_DAYS: i64 = 14;

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
