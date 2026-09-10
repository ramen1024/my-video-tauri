//! 视频扫描器 - Tauri 应用核心库
//!
//! 定义模块结构、Tauri Managed State（AppState），以及 Tauri Builder 配置。
//! 前端通过 Tauri IPC 调用 commands 模块中注册的命令，
//! 后端通过 server 模块提供局域网 HTTP 共享服务。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use parking_lot::{Mutex, RwLock};
use tauri::Manager;

mod commands;
mod constants;
mod error;
mod logging;
mod models;
mod password;
mod server;
mod utils;
mod video_cache;

#[cfg(test)]
mod test_utils;

pub use error::AppError;
pub use models::{ShareServerInfo, ShareStatus, VideoFile};
pub use password::PasswordStatus;

/// 初始化日志系统（main 入口调用）
pub fn init_logging() {
    logging::init();
}

/// HTTP 共享服务器的状态机
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServerState {
    /// 服务器已停止或尚未启动
    Stopped,
    /// 服务器正在启动中（扫描视频 + 绑定端口）
    Starting,
    /// 服务器正在运行，可接受请求
    Running,
    /// 服务器正在停止中（等待 worker 线程退出）
    Stopping,
}

/// 视频列表 ETag 缓存值：(列表 Arc 指针, 指纹)。指针一致时复用，避免每次请求全量计算
type VideosEtagCacheValue = (Arc<Vec<VideoFile>>, String);

/// Tauri 托管的应用状态
///
/// 替代原有的全局 static 变量，所有运行期状态都通过 Tauri 的 State 机制注入到命令中。
/// 内部字段均为 `Arc`，因此 `AppState` 可以在线程间 cheap clone。
#[derive(Clone)]
pub struct AppState {
    /// 当前共享的视频列表
    shared_videos: Arc<RwLock<Arc<Vec<VideoFile>>>>,
    /// 当前共享的文件夹路径
    shared_folder_path: Arc<RwLock<String>>,
    /// HTTP 共享服务器的运行状态
    server_state: Arc<Mutex<ServerState>>,
    /// tiny_http 服务器实例
    server_handle: Arc<RwLock<Option<Arc<tiny_http::Server>>>>,
    /// HTTP 服务器的 worker 线程句柄
    server_threads: Arc<RwLock<Vec<JoinHandle<()>>>>,
    /// 服务器运行时的对外信息（IP/端口），供前端重载后恢复界面
    share_info: Arc<RwLock<Option<ShareServerInfo>>>,
    /// 扫描取消标志：设为 true 时中止正在进行的视频扫描
    cancel_scan_flag: Arc<AtomicBool>,
    /// 扫描操作是否正在进行中（防止桌面扫描与网页刷新并发执行）
    scan_in_progress: Arc<AtomicBool>,
    /// 刷新操作是否正在进行中（防止并发刷新）
    refresh_in_progress: Arc<AtomicBool>,
    /// 刷新冷却标志（固定时间内不允许再次刷新）
    refresh_cooldown: Arc<AtomicBool>,
    /// 最近一次刷新操作的结果（JSON 字符串）
    refresh_result: Arc<RwLock<Option<String>>>,
    /// 视频列表 ETag 缓存：(列表 Arc 指针, 指纹)。指针一致时复用，避免每次请求全量计算
    videos_etag: Arc<RwLock<Option<VideosEtagCacheValue>>>,
    /// 密码保护状态（哈希、session、频率限制、配置目录）
    password: Arc<password::PasswordState>,
    /// 视频扫描结果缓存
    video_cache: Arc<Mutex<video_cache::VideoCache>>,
}

impl AppState {
    /// 创建默认的应用状态
    pub fn new() -> Self {
        Self {
            shared_videos: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            shared_folder_path: Arc::new(RwLock::new(String::new())),
            server_state: Arc::new(Mutex::new(ServerState::Stopped)),
            server_handle: Arc::new(RwLock::new(None)),
            server_threads: Arc::new(RwLock::new(Vec::new())),
            share_info: Arc::new(RwLock::new(None)),
            cancel_scan_flag: Arc::new(AtomicBool::new(false)),
            scan_in_progress: Arc::new(AtomicBool::new(false)),
            refresh_in_progress: Arc::new(AtomicBool::new(false)),
            refresh_cooldown: Arc::new(AtomicBool::new(false)),
            refresh_result: Arc::new(RwLock::new(None)),
            videos_etag: Arc::new(RwLock::new(None)),
            password: Arc::new(password::PasswordState::new()),
            video_cache: Arc::new(Mutex::new(video_cache::VideoCache::new(
                std::env::temp_dir(),
            ))),
        }
    }

    // ---------------- 视频与文件夹状态 ----------------

    /// 获取当前共享的视频列表
    pub fn shared_videos(&self) -> Arc<Vec<VideoFile>> {
        self.shared_videos.read().clone()
    }

    /// 设置共享的视频列表
    pub fn set_shared_videos(&self, videos: Vec<VideoFile>) {
        *self.shared_videos.write() = Arc::new(videos);
    }

    /// 获取当前共享的文件夹路径
    pub fn shared_folder_path(&self) -> String {
        self.shared_folder_path.read().clone()
    }

    /// 设置当前共享的文件夹路径
    pub fn set_shared_folder_path(&self, path: String) {
        *self.shared_folder_path.write() = path;
    }

    /// 获取当前视频列表的 ETag 指纹
    ///
    /// 以列表的 Arc 指针作为缓存键：列表对象未更换时直接复用已计算的指纹；
    /// 更换（扫描/刷新写入新列表）时自动重算，保证指纹与列表严格对应，不会返回陈旧值。
    pub fn videos_etag(&self) -> String {
        let current = self.shared_videos();
        {
            let guard = self.videos_etag.read();
            if let Some((ref cached_arc, ref etag)) = *guard {
                if Arc::ptr_eq(cached_arc, &current) {
                    return etag.clone();
                }
            }
        }
        let etag = crate::utils::compute_videos_etag(&current);
        *self.videos_etag.write() = Some((current, etag.clone()));
        etag
    }

    // ---------------- 密码保护 ----------------

    /// 获取密码保护状态（引用，用于状态查询与方法调用）
    pub fn password(&self) -> &password::PasswordState {
        &self.password
    }

    /// 获取密码保护状态的 Arc 引用（用于后台清理线程持有）
    pub fn password_arc(&self) -> Arc<password::PasswordState> {
        self.password.clone()
    }

    // ---------------- 视频扫描缓存 ----------------

    /// 获取视频缓存的内部引用
    pub fn video_cache(&self) -> &Arc<Mutex<video_cache::VideoCache>> {
        &self.video_cache
    }

    /// 重新设置视频缓存目录（例如使用 app_data_dir）
    pub fn set_video_cache_dir(&self, cache_dir: PathBuf) {
        self.video_cache.lock().set_cache_dir(cache_dir);
    }

    // ---------------- 扫描取消标志 ----------------

    /// 重置扫描取消标志
    pub fn reset_cancel_scan_flag(&self) {
        self.cancel_scan_flag.store(false, Ordering::Relaxed);
    }

    /// 设置扫描取消标志
    pub fn set_cancel_scan_flag(&self) {
        self.cancel_scan_flag.store(true, Ordering::Relaxed);
    }

    /// 检查扫描是否已被取消
    pub fn is_scan_cancelled(&self) -> bool {
        self.cancel_scan_flag.load(Ordering::Relaxed)
    }

    // ---------------- 扫描互斥 ----------------

    /// 尝试标记扫描开始，返回是否成功
    ///
    /// 桌面扫描与网页刷新共用此标记，保证两者不会并发写入 shared_videos。
    pub fn start_scan(&self) -> bool {
        self.scan_in_progress
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// 标记扫描结束
    pub fn finish_scan(&self) {
        self.scan_in_progress.store(false, Ordering::Release);
    }

    // ---------------- 服务器状态机 ----------------

    /// 获取服务器当前状态
    pub fn server_state(&self) -> ServerState {
        *self.server_state.lock()
    }

    /// 检查服务器是否处于运行状态
    pub fn is_server_running(&self) -> bool {
        self.server_state() == ServerState::Running
    }

    /// 尝试进入 Starting 状态，失败时返回对应错误
    pub fn start_server_starting(&self) -> Result<(), AppError> {
        let mut state = self.server_state.lock();
        match *state {
            ServerState::Running | ServerState::Starting => {
                Err(AppError::ServerAlreadyRunning("服务器已在运行".to_string()))
            }
            ServerState::Stopping => Err(AppError::Other("服务器正在停止中，请稍后".to_string())),
            ServerState::Stopped => {
                *state = ServerState::Starting;
                Ok(())
            }
        }
    }

    /// 将服务器设置为运行状态，并保存服务器实例、worker 线程与对外信息
    pub fn set_server_running(
        &self,
        server: Arc<tiny_http::Server>,
        threads: Vec<JoinHandle<()>>,
        info: ShareServerInfo,
    ) {
        let mut handle = self.server_handle.write();
        let mut worker_threads = self.server_threads.write();
        let mut state = self.server_state.lock();
        *handle = Some(server);
        *worker_threads = threads;
        *state = ServerState::Running;
        *self.share_info.write() = Some(info);
    }

    /// 将服务器设置为 Stopping 状态，并返回当前 worker 数量
    ///
    /// 加锁顺序（threads → state）与 [`Self::set_server_running`] /
    /// [`Self::set_server_stopped`] 保持一致，避免并发启停时出现 ABBA 死锁。
    pub fn start_server_stopping(&self) -> Result<usize, AppError> {
        let worker_count = self.server_threads.read().len();
        let mut state = self.server_state.lock();
        match *state {
            ServerState::Stopped | ServerState::Stopping => {
                Err(AppError::ServerNotRunning("服务器未运行".to_string()))
            }
            ServerState::Starting => Err(AppError::Other("服务器正在启动中，请稍后".to_string())),
            ServerState::Running => {
                *state = ServerState::Stopping;
                Ok(worker_count)
            }
        }
    }

    /// 将服务器完全置为停止状态，并清空服务器句柄与线程记录
    pub fn set_server_stopped(&self) {
        let mut handle = self.server_handle.write();
        let mut threads = self.server_threads.write();
        let mut state = self.server_state.lock();
        *handle = None;
        *threads = Vec::new();
        *state = ServerState::Stopped;
        *self.share_info.write() = None;
    }

    /// 汇总当前共享状态，供前端在 webview 重载后恢复界面
    ///
    /// 注意先克隆出 `share_info` 再查询服务器状态，避免持有 `share_info`
    /// 的同时去取服务器状态锁，与写入路径形成相反的加锁顺序。
    pub fn share_status(&self) -> ShareStatus {
        let info = self.share_info.read().clone();
        ShareStatus {
            running: self.is_server_running(),
            ips: info.as_ref().map(|i| i.ips.clone()).unwrap_or_default(),
            port: info.as_ref().map(|i| i.port).unwrap_or(0),
            folder_path: self.shared_folder_path(),
        }
    }

    /// 取出当前服务器句柄（用于停止时 unblock）
    pub fn take_server_handle(&self) -> Option<Arc<tiny_http::Server>> {
        self.server_handle.write().take()
    }

    /// 取出当前所有 worker 线程句柄
    pub fn take_server_threads(&self) -> Vec<JoinHandle<()>> {
        std::mem::take(&mut *self.server_threads.write())
    }

    // ---------------- 刷新状态 ----------------

    /// 获取最近一次刷新结果
    pub fn refresh_result(&self) -> Option<String> {
        self.refresh_result.read().clone()
    }

    /// 设置刷新结果
    pub fn set_refresh_result(&self, result: String) {
        *self.refresh_result.write() = Some(result);
    }

    /// 清空刷新结果
    pub fn clear_refresh_result(&self) {
        *self.refresh_result.write() = None;
    }

    /// 检查是否有刷新正在进行
    pub fn is_refresh_in_progress(&self) -> bool {
        self.refresh_in_progress.load(Ordering::Acquire)
    }

    /// 尝试标记刷新开始，返回是否成功
    pub fn start_refresh(&self) -> bool {
        self.refresh_in_progress
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// 标记刷新结束
    pub fn finish_refresh(&self) {
        self.refresh_in_progress.store(false, Ordering::Release);
    }

    /// 检查是否处于刷新冷却期
    pub fn is_refresh_cooldown(&self) -> bool {
        self.refresh_cooldown.load(Ordering::Acquire)
    }

    /// 尝试标记刷新冷却开始，返回是否成功
    pub fn start_refresh_cooldown(&self) -> bool {
        self.refresh_cooldown
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// 标记刷新冷却结束
    pub fn finish_refresh_cooldown(&self) {
        self.refresh_cooldown.store(false, Ordering::Release);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tauri 应用入口函数
///
/// 配置并启动 Tauri 应用，包括：
/// - 注册插件（opener、dialog、导航守卫）
/// - 初始化密码配置和清理线程
/// - 注册 Tauri Managed State（AppState）
/// - 注册所有 IPC 命令处理器
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("navigation-guard")
                .on_navigation(|_webview, url| {
                    let url_str = url.as_str();
                    let allowed = url_str.starts_with("http://localhost:1420")
                        || url_str.starts_with("http://localhost:1421")
                        || url_str.starts_with("https://tauri.localhost")
                        || url_str.starts_with("http://tauri.localhost")
                        || url_str.starts_with("tauri://")
                        || url.scheme() == "asset";
                    if !allowed {
                        log::warn!("[导航拦截] 阻止外部导航: {}", url_str);
                    }
                    allowed
                })
                .build(),
        )
        .setup(|app| {
            let app_state = AppState::new();

            if let Ok(data_dir) = app.path().app_data_dir() {
                app_state.password().set_config_dir(data_dir.clone());
                app_state.set_video_cache_dir(data_dir.clone());
                logging::set_log_file(data_dir);
            }

            app_state.password().load_config();
            password::start_cleanup_thread(app_state.password_arc());
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::video::scan_videos,
            commands::video::get_shared_videos,
            commands::video::play_video,
            commands::video::cancel_scan,
            commands::share::start_share_server,
            commands::share::stop_share_server,
            commands::share::get_share_status,
            commands::password_cmd::get_password_status,
            commands::password_cmd::set_password_enabled,
            commands::password_cmd::set_password,
            commands::password_cmd::generate_random_password,
            commands::password_cmd::reset_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
