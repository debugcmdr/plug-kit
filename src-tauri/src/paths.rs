//! 数据目录路径集中管理(G-6):所有 `~/.plugkit` 相关路径唯一定义于此。
//!
//! 此前数据/日志/缓存/工作目录路径分散在 lib.rs / logger.rs / dependency_cache.rs /
//! plugin_manager.rs / preinstall.rs / tool_runner.rs / config_mgr.rs / task_queue.rs /
//! message_bridge.rs 多处,各写各的 `dirs::home_dir().join(".plugkit/...")`。
//! 集中后:未来支持自定义数据目录(便携模式等)只需改本模块;新增子目录也不再散落。
//!
//! ⚠️ 插件侧(Python shared)仍有 `~/.plugkit/venvs/ytdlp` 等路径,改动本模块
//! 时需同步 _ytdlp.py 的 VENV_DIR。

use std::path::PathBuf;

/// 数据根目录 `~/.plugkit`。
fn data_root() -> PathBuf {
    dirs::home_dir()
        .map(|d| d.join(".plugkit"))
        .unwrap_or_default()
}

/// 已安装插件目录 `~/.plugkit/plugins`。
pub fn plugins_dir() -> PathBuf {
    data_root().join("plugins")
}

/// 日志目录 `~/.plugkit/logs`。
pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

/// 任务持久化文件 `~/.plugkit/tasks/tasks.json`。
pub fn tasks_file() -> PathBuf {
    data_root().join("tasks").join("tasks.json")
}

/// 插件配置根目录 `~/.plugkit/configs`。
pub fn configs_dir() -> PathBuf {
    data_root().join("configs")
}

/// 依赖缓存根目录 `~/.plugkit/cache/deps`。
pub fn cache_deps_dir() -> PathBuf {
    data_root().join("cache").join("deps")
}

/// 任务工作目录 `~/.plugkit/work`(与插件安装临时目录分离,防误删)。
pub fn work_dir() -> PathBuf {
    data_root().join("work")
}

/// 插件安装临时目录 `~/.plugkit/tmp`。
pub fn tmp_dir() -> PathBuf {
    data_root().join("tmp")
}
