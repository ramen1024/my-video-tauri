use std::path::Path;

use crate::error::AppError;
use crate::models::ShareServerInfo;
use crate::utils::get_local_ips;
use crate::server;
use crate::{SERVER_HANDLE, SERVER_STATE, SERVER_THREADS, ServerState};

#[tauri::command]
pub async fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, AppError> {
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

    log::info!("[共享] 开始启动: folder={}, port={}", folder_path, port);

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
        return Err(AppError::InvalidPath("无效的文件夹路径".to_string()));
    }

    let folder_path_clone = folder_path.clone();
    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        super::video::scan_videos_sync(folder_path_clone)
    })
    .await
    .map_err(|e| {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
        log::error!("[共享] 扫描任务执行失败: {}", e);
        AppError::Other(format!("扫描任务执行失败: {}", e))
    })?;

    if let Err(e) = scan_result {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
        log::error!("[共享] 扫描失败: {}", e);
        return Err(e);
    }

    log::info!("[共享] 视频扫描完成，正在启动HTTP服务器...");

    let ips = get_local_ips();
    let ips_clone = ips.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = server::start_http_server(&ips_clone, port);
        let _ = tx.send(result);
    });

    let server_result = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(10))
    })
    .await
    .map_err(|e| {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
        log::error!("[共享] 等待服务器启动失败: {}", e);
        AppError::Other(format!("等待服务器启动失败: {}", e))
    })?;

    match server_result {
        Ok(Ok((server_arc, handles))) => {
            {
                let mut state = SERVER_STATE.lock();
                *state = ServerState::Running;
            }
            SERVER_HANDLE.write().replace(server_arc);
            let mut threads = SERVER_THREADS.write();
            *threads = handles;
            log::info!("[共享] 服务器启动成功: ips={:?}, port={}", ips, port);
            Ok(ShareServerInfo { ips, port })
        }
        Ok(Err(e)) => {
            let mut state = SERVER_STATE.lock();
            *state = ServerState::Stopped;
            log::error!("[共享] HTTP服务器启动失败: {}", e);
            Err(AppError::IoError(e))
        }
        Err(_) => {
            let mut state = SERVER_STATE.lock();
            *state = ServerState::Stopped;
            log::error!("[共享] 服务器启动超时");
            Err(AppError::IoError("服务器启动超时".to_string()))
        }
    }
}

#[tauri::command]
pub async fn stop_share_server() -> Result<(), AppError> {
    let worker_count = {
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
                log::info!("[共享] 开始停止服务器, worker_count={}", SERVER_THREADS.read().len());
                SERVER_THREADS.read().len()
            }
        }
    };

    if let Some(server) = SERVER_HANDLE.write().take() {
        for _ in 0..worker_count.max(1) {
            server.unblock();
        }
        drop(server);
    }

    let handles = {
        let mut threads = SERVER_THREADS.write();
        std::mem::take(&mut *threads)
    };

    if !handles.is_empty() {
        std::thread::spawn(move || {
            for handle in handles {
                let _ = handle.join();
            }
            log::info!("[共享] 所有worker线程已退出");
        });
    }

    {
        let mut state = SERVER_STATE.lock();
        *state = ServerState::Stopped;
    }

    log::info!("[共享] 服务器已停止");
    Ok(())
}

#[tauri::command]
pub fn get_server_status() -> bool {
    matches!(*SERVER_STATE.lock(), ServerState::Running)
}
