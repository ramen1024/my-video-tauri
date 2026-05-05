//! 视频扫描器 - Tauri 应用核心库
//!
//! 定义全局状态、模块结构，以及 Tauri Builder 配置。
//! 前端通过 Tauri IPC 调用 commands 模块中注册的命令，
//! 后端通过 server 模块提供局域网 HTTP 共享服务。

use std::sync::LazyLock;

use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

mod error;
mod models;
mod password;
mod utils;
pub mod commands;
mod server;

pub use error::AppError;
pub use models::{ShareServerInfo, VideoFile};
pub use password::PasswordStatus;

/// 扫描取消标志：设为 true 时中止正在进行的视频扫描
pub(crate) static CANCEL_SCAN_FLAG: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(false));

/// 当前共享的视频列表，扫描完成后写入此全局状态
pub(crate) static SHARED_VIDEOS: LazyLock<RwLock<Arc<Vec<VideoFile>>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Vec::new())));

/// 当前共享的文件夹路径，HTTP 服务器据此定位视频文件
pub(crate) static SHARED_FOLDER_PATH: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(String::new()));

/// HTTP 共享服务器的运行状态
pub(crate) static SERVER_STATE: LazyLock<Mutex<ServerState>> =
    LazyLock::new(|| Mutex::new(ServerState::Stopped));

/// tiny_http 服务器实例，停止时需要调用 unblock 通知各 worker 退出
pub(crate) static SERVER_HANDLE: LazyLock<RwLock<Option<Arc<tiny_http::Server>>>> =
    LazyLock::new(|| RwLock::new(None));

/// HTTP 服务器的 worker 线程句柄，停止时等待它们全部退出
pub(crate) static SERVER_THREADS: LazyLock<RwLock<Vec<std::thread::JoinHandle<()>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// HTTP 共享服务器的状态机
pub(crate) enum ServerState {
    /// 服务器已停止或尚未启动
    Stopped,
    /// 服务器正在启动中（扫描视频 + 绑定端口）
    Starting,
    /// 服务器正在运行，可接受请求
    Running,
    /// 服务器正在停止中（等待 worker 线程退出）
    Stopping,
}

/// Tauri 应用入口函数
///
/// 配置并启动 Tauri 应用，包括：
/// - 注册插件（opener、dialog、导航守卫）
/// - 初始化密码配置和清理线程
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
            if let Ok(data_dir) = app.path().app_data_dir() {
                password::set_config_dir(data_dir);
            }
            password::load_password_config();
            password::start_cleanup_thread();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::video::scan_videos,
            commands::video::get_shared_videos,
            commands::video::play_video,
            commands::video::cancel_scan,
            commands::share::start_share_server,
            commands::share::stop_share_server,
            commands::share::get_server_status,
            commands::password_cmd::get_password_status,
            commands::password_cmd::set_password_enabled,
            commands::password_cmd::set_password,
            commands::password_cmd::verify_password_cmd,
            commands::password_cmd::generate_random_password,
            commands::password_cmd::reset_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
