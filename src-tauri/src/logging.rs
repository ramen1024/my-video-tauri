//! 双写日志模块（控制台 + 文件）
//!
//! Release 模式下 Windows 应用没有控制台窗口，env_logger 的输出不可见，
//! 出问题时难以排查。本模块在 env_logger 基础上追加文件输出：
//! - 日志路径在 Tauri setup 阶段通过 `set_log_file` 设置为应用数据目录
//! - 文件超过 5MB 时轮转为 `.1` 后缀的旧文件
//! - 打开文件失败仅降级为控制台输出，不影响应用运行

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use log::{LevelFilter, Log, Metadata, Record};

/// 日志文件大小上限（超过后轮转）
const MAX_LOG_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// 日志文件句柄（由 set_log_file 打开，log 时追加写入）
static LOG_FILE: LazyLock<Mutex<Option<File>>> = LazyLock::new(|| Mutex::new(None));

/// 应用日志器：委托 env_logger 输出控制台，同时追加写入日志文件
struct DualLogger {
    console: env_logger::Logger,
}

impl Log for DualLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.console.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if !self.console.enabled(record.metadata()) {
            return;
        }
        self.console.log(record);
        if let Ok(mut file_guard) = LOG_FILE.lock() {
            if let Some(file) = file_guard.as_mut() {
                let line = format!(
                    "[{}] [{}] {}\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    record.args()
                );
                let _ = file.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        self.console.flush();
        if let Ok(mut file_guard) = LOG_FILE.lock() {
            if let Some(file) = file_guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// 初始化日志系统（在 main 中调用，替代 env_logger 直接初始化）
pub fn init() {
    let console = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .build();
    log::set_boxed_logger(Box::new(DualLogger { console })).expect("设置日志器失败");
    log::set_max_level(LevelFilter::Info);
}

/// 设置日志文件目录（Tauri setup 阶段调用）
///
/// 日志写入 `<dir>/video-scanner.log`，文件超过大小上限时轮转为 `.1` 后缀。
/// 打开失败仅记警告，不影响应用运行。
pub fn set_log_file(dir: PathBuf) {
    let path = dir.join("video-scanner.log");
    let file = open_rotated_log(&path);
    if let Ok(mut guard) = LOG_FILE.lock() {
        *guard = file;
    }
}

/// 打开日志文件；若已存在且超过大小上限，先轮转为 `.1` 后缀
fn open_rotated_log(path: &Path) -> Option<File> {
    let _ = std::fs::create_dir_all(path.parent()?);

    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > MAX_LOG_FILE_SIZE {
            let old_path = path.with_extension("log.1");
            let _ = std::fs::rename(path, old_path);
        }
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            log::warn!("[日志] 打开日志文件失败，仅输出到控制台: {} 路径: {:?}", e, path);
        })
        .ok()
}
