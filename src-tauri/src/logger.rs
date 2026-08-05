use std::fs;
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;

pub struct Logger;

impl Logger {
    pub fn info(msg: &str) {
        Self::write_log("INFO", msg);
    }

    pub fn error(msg: &str) {
        Self::write_log("ERROR", msg);
    }

    pub fn warn(msg: &str) {
        Self::write_log("WARN", msg);
    }

    fn write_log(level: &str, msg: &str) {
        let log_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".multitool")
            .join("logs");
        let log_file = log_dir.join(format!("{}.log", Local::now().format("%Y-%m-%d")));

        if let Some(parent) = log_file.parent() {
            let _ = fs::create_dir_all(parent);
        }

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
}
