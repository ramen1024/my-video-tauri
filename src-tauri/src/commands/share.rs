use std::path::Path;

use crate::error::AppError;
use crate::models::ShareServerInfo;
use crate::utils::get_local_ips;
use crate::server;
use crate::{SERVER_STATE, SERVER_THREADS, ServerState};

#[tauri::command]
pub fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, AppError> {
    {
        let mut state = SERVER_STATE.lock();
        match *state {
            ServerState::Running | ServerState::Starting => {
                return Err(AppError::ServerAlreadyRunning);
            }
            ServerState::Stopping => {
                return Err(AppError::Other("服务器正在停止中，请稍后".to_string()));
            }
            ServerState::Stopped => {
                *state = ServerState::Starting;
            }
        }
    }

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
        return Err(AppError::InvalidPath("无效的文件夹路径".to_string()));
    }

    super::video::scan_videos_sync(folder_path.clone())?;
    let ips = get_local_ips();

    let ips_clone = ips.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = server::start_http_server(&ips_clone, port);
        let _ = tx.send(result);
    });

    match rx.recv() {
        Ok(Ok(handles)) => {
            let mut state = SERVER_STATE.lock();
            *state = ServerState::Running;
            let mut threads = SERVER_THREADS.write();
            *threads = handles;
        }
        Ok(Err(e)) => {
            let mut state = SERVER_STATE.lock();
            *state = ServerState::Stopped;
            return Err(AppError::IoError(e));
        }
        Err(_) => {
            let mut state = SERVER_STATE.lock();
            *state = ServerState::Stopped;
            return Err(AppError::IoError("服务器线程通信失败".to_string()));
        }
    }

    Ok(ShareServerInfo { ips, port })
}

#[tauri::command]
pub fn stop_share_server() -> Result<(), AppError> {
    {
        let mut state = SERVER_STATE.lock();
        match *state {
            ServerState::Stopped | ServerState::Stopping => {
                return Err(AppError::ServerNotRunning);
            }
            ServerState::Starting => {
                return Err(AppError::Other("服务器正在启动中，请稍后".to_string()));
            }
            ServerState::Running => {
                *state = ServerState::Stopping;
            }
        }
    }

    if let Some(server) = crate::SERVER_HANDLE.write().take() {
        server.unblock();
    }

    let handles = {
        let mut threads = SERVER_THREADS.write();
        std::mem::take(&mut *threads)
    };

    for handle in handles {
        let _ = handle.join();
    }

    {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
    }

    Ok(())
}

#[tauri::command]
pub fn get_server_status() -> bool {
    matches!(*SERVER_STATE.lock(), ServerState::Running)
}
