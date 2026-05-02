use std::path::Path;
use std::sync::atomic::Ordering;

use crate::error::AppError;
use crate::models::ShareServerInfo;
use crate::utils::get_local_ips;
use crate::server;
use crate::{SERVER_RUNNING, SHARED_VIDEOS};

#[tauri::command]
pub fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, AppError> {
    if SERVER_RUNNING.load(Ordering::SeqCst) {
        return Err(AppError::ServerAlreadyRunning);
    }

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath("无效的文件夹路径".to_string()));
    }

    super::video::scan_videos(folder_path.clone())?;
    let videos = SHARED_VIDEOS.read().clone();
    let ips = get_local_ips();

    let ips_clone = ips.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = server::start_http_server(&ips_clone, port);
        let _ = tx.send(result);
    });

    match rx.recv() {
        Ok(Ok(())) => {
            SERVER_RUNNING.store(true, Ordering::SeqCst);
        }
        Ok(Err(e)) => {
            return Err(AppError::IoError(e));
        }
        Err(_) => {
            return Err(AppError::IoError("服务器线程通信失败".to_string()));
        }
    }

    Ok(ShareServerInfo { ips, port, videos })
}

#[tauri::command]
pub fn stop_share_server() -> Result<(), AppError> {
    SERVER_RUNNING.store(false, Ordering::SeqCst);
    crate::SERVER_HANDLE.write().take().map(|s| s.unblock());
    Ok(())
}

#[tauri::command]
pub fn get_server_status() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}
