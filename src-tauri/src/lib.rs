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

pub(crate) static CANCEL_SCAN_FLAG: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(false));

pub(crate) static SHARED_VIDEOS: LazyLock<RwLock<Arc<Vec<VideoFile>>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Vec::new())));

pub(crate) static SHARED_FOLDER_PATH: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(String::new()));

pub(crate) static SERVER_STATE: LazyLock<Mutex<ServerState>> =
    LazyLock::new(|| Mutex::new(ServerState::Stopped));

pub(crate) static SERVER_HANDLE: LazyLock<RwLock<Option<Arc<tiny_http::Server>>>> =
    LazyLock::new(|| RwLock::new(None));

pub(crate) enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
