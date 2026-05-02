use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use parking_lot::RwLock;
use once_cell::sync::Lazy;

mod error;
mod models;
mod password;
mod utils;
pub mod commands;
mod server;

pub use error::AppError;
pub use models::{ShareServerInfo, VideoFile};
pub use password::PasswordStatus;

pub(crate) static CANCEL_SCAN_FLAG: Lazy<AtomicBool> =
    Lazy::new(|| AtomicBool::new(false));

pub(crate) static SHARED_VIDEOS: Lazy<RwLock<Vec<VideoFile>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

pub(crate) static SHARED_FOLDER_PATH: Lazy<RwLock<String>> =
    Lazy::new(|| RwLock::new(String::new()));

pub(crate) static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

pub(crate) static SERVER_HANDLE: Lazy<RwLock<Option<Arc<tiny_http::Server>>>> =
    Lazy::new(|| RwLock::new(None));

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    password::load_password_config();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
