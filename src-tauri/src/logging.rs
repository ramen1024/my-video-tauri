//! 双写日志模块（控制台 + 文件）
//!
//! Release 模式下 Windows 应用没有控制台窗口，env_logger 的输出不可见，
//! 出问题时难以排查。本模块在 env_logger 基础上追加文件输出：
//! - 日志路径在 Tauri setup 阶段通过 `set_log_file` 设置为应用数据目录
//! - 文件超过 5MB 时轮转为 `.1` 后缀的旧文件（启动时与**每次写入后**都会检查，
//!   因此长时间运行的会话也不会无限增长）
//! - 打开文件失败仅降级为控制台输出，不影响应用运行

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use log::{Log, Metadata, Record};

/// 日志文件大小上限（超过后轮转）
const MAX_LOG_FILE_SIZE: u64 = 5 * 1024 * 1024;

/// 轮转后的旧日志后缀（`video-scanner.log` → `video-scanner.log.1`）
const ROTATED_EXTENSION: &str = "log.1";

/// 已打开的日志文件及其状态
struct LogFile {
    file: File,
    path: PathBuf,
    /// 已写入的字节数
    ///
    /// 记录计数而不是每次写入都 `stat`：日志写入在热路径上，避免额外的系统调用。
    written: u64,
}

/// 日志文件句柄（由 set_log_file 打开，log 时追加写入）
static LOG_FILE: LazyLock<Mutex<Option<LogFile>>> = LazyLock::new(|| Mutex::new(None));

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
            if let Some(log_file) = file_guard.as_mut() {
                let line = format!(
                    "[{}] [{}] {}\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    record.level(),
                    record.args()
                );
                if log_file.file.write_all(line.as_bytes()).is_ok() {
                    log_file.written += line.len() as u64;
                }
                rotate_if_needed(log_file);
            }
        }
    }

    fn flush(&self) {
        self.console.flush();
        if let Ok(mut file_guard) = LOG_FILE.lock() {
            if let Some(log_file) = file_guard.as_mut() {
                let _ = log_file.file.flush();
            }
        }
    }
}

/// 初始化日志系统（在 main 中调用，替代 env_logger 直接初始化）
pub fn init() {
    let console =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    // 全局上限必须取自 env 的实际过滤结果（与 env_logger::init 的做法一致）：
    // 写死 Info 会在记录进入 logger 之前就丢弃 debug/trace，使 RUST_LOG=debug 完全失效
    let max_level = console.filter();
    log::set_boxed_logger(Box::new(DualLogger { console })).expect("设置日志器失败");
    log::set_max_level(max_level);
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
fn open_rotated_log(path: &Path) -> Option<LogFile> {
    let _ = std::fs::create_dir_all(path.parent()?);

    let existing_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if existing_size > MAX_LOG_FILE_SIZE {
        rotate_file(path);
    }

    // 轮转成功后原文件已不存在（或本就不存在），重新打开即可
    let reopened_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            log::warn!(
                "[日志] 打开日志文件失败，仅输出到控制台: {} 路径: {:?}",
                e,
                path
            );
        })
        .ok()?;

    Some(LogFile {
        file,
        path: path.to_path_buf(),
        written: reopened_size,
    })
}

/// 把 `path` 轮转为 `.1` 后缀的旧文件（覆盖上一次的旧文件）
fn rotate_file(path: &Path) {
    let rotated = path.with_extension(ROTATED_EXTENSION);
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(path, rotated);
}

/// 超出大小上限时轮转并重新打开日志文件
///
/// 注意：本函数在持有 `LOG_FILE` 锁的情况下被调用，因此**不能**使用 `log::warn!`
/// （std 的 `Mutex` 不可重入，会自锁死），失败信息改用 `eprintln!` 输出。
fn rotate_if_needed(log_file: &mut LogFile) {
    if log_file.written <= MAX_LOG_FILE_SIZE {
        return;
    }

    let _ = log_file.file.flush();
    rotate_file(&log_file.path);

    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file.path)
    {
        Ok(file) => {
            log_file.file = file;
            log_file.written = 0;
        }
        Err(e) => {
            eprintln!(
                "[日志] 轮转后重新打开日志文件失败，后续不再写文件: {} 路径: {:?}",
                e, log_file.path
            );
            // 无法重开时保留旧句柄并重置计数，避免每条日志都重复尝试轮转
            log_file.written = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::make_temp_dir;

    /// 打开一个日志文件并写入指定字节数
    fn open_with_bytes(path: &Path, bytes: usize) -> LogFile {
        std::fs::write(path, vec![b'x'; bytes]).expect("写入测试日志失败");
        open_rotated_log(path).expect("应能打开日志文件")
    }

    #[test]
    fn open_rotated_log_rotates_oversized_file_at_startup() {
        let dir = make_temp_dir("logging_startup_rotate");
        let path = dir.join("video-scanner.log");
        let log_file = open_with_bytes(&path, MAX_LOG_FILE_SIZE as usize + 1);

        // 原文件被移走，轮转文件保留旧内容
        assert_eq!(log_file.written, 0, "轮转后写入计数应归零");
        assert!(path.exists(), "应重新创建日志文件");
        assert_eq!(
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            0,
            "新日志文件应为空"
        );

        let rotated = path.with_extension(ROTATED_EXTENSION);
        assert!(rotated.exists(), "应生成 .log.1 旧日志");
        assert_eq!(
            std::fs::metadata(&rotated).map(|m| m.len()).unwrap_or(0),
            MAX_LOG_FILE_SIZE + 1,
            "旧日志应完整保留"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_rotated_log_keeps_small_file() {
        let dir = make_temp_dir("logging_small_file");
        let path = dir.join("video-scanner.log");
        let log_file = open_with_bytes(&path, 128);

        assert_eq!(log_file.written, 128, "未超限时不应轮转，计数应为已有大小");
        assert!(
            !path.with_extension(ROTATED_EXTENSION).exists(),
            "未超限时不应产生 .log.1"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotate_if_needed_rotates_only_after_limit() {
        let dir = make_temp_dir("logging_runtime_rotate");
        let path = dir.join("video-scanner.log");
        let mut log_file = open_with_bytes(&path, 0);

        // 未超限：不轮转
        log_file.written = MAX_LOG_FILE_SIZE;
        rotate_if_needed(&mut log_file);
        assert!(
            !path.with_extension(ROTATED_EXTENSION).exists(),
            "恰好等于上限时不应轮转"
        );

        // 超限：轮转并重置计数
        log_file.written = MAX_LOG_FILE_SIZE + 1;
        rotate_if_needed(&mut log_file);
        assert!(
            path.with_extension(ROTATED_EXTENSION).exists(),
            "应生成 .log.1"
        );
        assert_eq!(log_file.written, 0, "轮转后计数应归零");
        assert!(path.exists(), "应重新创建日志文件");

        // 轮转后的文件仍可继续写入
        log_file
            .file
            .write_all(b"after rotate\n")
            .expect("写入失败");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
