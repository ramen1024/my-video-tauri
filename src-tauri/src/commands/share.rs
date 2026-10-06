//! 局域网共享服务器命令
//!
//! 提供共享服务器的启动、停止和状态查询等 Tauri IPC 命令。
//! 启动服务器时会先扫描视频，然后启动 HTTP 服务器监听指定端口。

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use tauri::State;

use crate::constants::{MAX_PORT_ATTEMPTS, SERVER_START_TIMEOUT_SECS, SERVER_STOP_TIMEOUT_SECS};
use crate::error::AppError;
use crate::models::{ShareServerInfo, ShareStatus};
use crate::server::{self, StopSignal};
use crate::utils::{allow_shared_folder_asset_scope, get_local_ips};
use crate::AppState;

/// 启动局域网共享服务器
///
/// 流程：状态检查 → 扫描视频 → 启动 HTTP 服务器（端口被占用时自动尝试下一个）→ 等待就绪
/// 服务器在独立线程中运行，通过 channel 通知启动结果
#[tauri::command]
pub async fn start_share_server(
    folder_path: String,
    port: u16,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ShareServerInfo, AppError> {
    let app_state = state.inner().clone();

    app_state.start_server_starting()?;

    log::info!("[共享] 开始启动: folder={}, port={}", folder_path, port);

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        app_state.set_server_stopped();
        return Err(AppError::InvalidPath("无效的文件夹路径".to_string()));
    }

    let folder_path_clone = folder_path.clone();
    let scan_app_state = app_state.clone();
    let scan_result = match tauri::async_runtime::spawn_blocking(move || {
        super::video::scan_videos_sync(folder_path_clone, &scan_app_state)
    })
    .await
    {
        Ok(res) => res,
        Err(e) => {
            app_state.set_server_stopped();
            log::error!("[共享] 扫描任务执行失败: {}", e);
            return Err(AppError::Other(format!("扫描任务执行失败: {}", e)));
        }
    };

    if let Err(e) = scan_result {
        app_state.set_server_stopped();
        log::error!("[共享] 扫描失败: {}", e);
        return Err(e);
    }

    log::info!("[共享] 视频扫描完成，正在启动HTTP服务器...");

    let ips = get_local_ips();

    // 端口被占用时自动尝试下一个端口，最多 MAX_PORT_ATTEMPTS 个
    let mut last_error = "服务器启动失败".to_string();
    let mut server_handle: Option<Arc<server::RunningServer>> = None;
    let mut worker_handles: Vec<JoinHandle<()>> = Vec::new();
    let mut started_port = port;

    for attempt in 0..MAX_PORT_ATTEMPTS {
        let candidate = port.saturating_add(attempt);
        let (tx, rx) = std::sync::mpsc::channel();
        // 每次尝试使用独立的停止信号：只有本次绑定成功并真正投入运行时才会被保留
        let stop_signal = StopSignal::new();

        let server_app_state = Arc::new(app_state.clone());
        let ips_clone = ips.clone();
        let signal_for_thread = stop_signal.clone();
        std::thread::spawn(move || {
            let result = server::start_http_server(
                &ips_clone,
                candidate,
                server_app_state,
                signal_for_thread,
            );
            let _ = tx.send(result);
        });

        let server_result = match tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(SERVER_START_TIMEOUT_SECS))
        })
        .await
        {
            Ok(res) => res,
            Err(e) => {
                log::error!("[共享] 等待服务器启动失败: {}", e);
                stop_signal.stop();
                last_error = format!("等待服务器启动失败: {}", e);
                continue;
            }
        };

        match server_result {
            Ok(Ok((running, handles))) => {
                server_handle = Some(Arc::new(running));
                worker_handles = handles;
                started_port = candidate;
                break;
            }
            Ok(Err(e)) => {
                log::warn!("[共享] 端口 {} 启动失败: {}", candidate, e);
                stop_signal.stop();
                last_error = e;
            }
            Err(_) => {
                // 等待超时：启动线程可能仍在推进，置停止信号保证它即使随后绑定成功，
                // worker 也会立即退出，不会留下无人跟踪、却占着端口的孤儿实例
                log::warn!("[共享] 端口 {} 启动超时", candidate);
                stop_signal.stop();
                last_error = "服务器启动超时".to_string();
            }
        }
    }

    match server_handle {
        Some(running) => {
            let info = ShareServerInfo {
                ips,
                port: started_port,
            };
            app_state.set_server_running(running, worker_handles, info.clone());
            // 桌面端播放视频需要 asset 协议访问该文件夹
            allow_shared_folder_asset_scope(&app, &folder_path);
            log::info!(
                "[共享] 服务器启动成功: ips={:?}, port={}",
                info.ips,
                info.port
            );
            Ok(info)
        }
        None => {
            app_state.set_server_stopped();
            log::error!("[共享] HTTP服务器启动失败: {}", last_error);
            Err(AppError::IoError(last_error))
        }
    }
}

/// 停止局域网共享服务器
///
/// 先置停止信号（worker 在循环顶判断），再 unblock 唤醒阻塞在 recv 的 worker；
/// 等待它们完全结束后再释放监听套接字。
/// 只有等所有 worker 线程退出、tiny_http Server 的 Arc 引用归零，
/// 操作系统才会真正释放监听端口，避免再次启动时出现 "地址已在使用" 错误。
#[tauri::command]
pub async fn stop_share_server(state: State<'_, AppState>) -> Result<(), AppError> {
    let app_state = state.inner().clone();

    let worker_count = app_state.start_server_stopping()?;
    log::info!("[共享] 开始停止服务器, worker_count={}", worker_count);

    if let Some(running) = app_state.take_server_handle() {
        running.shutdown(worker_count);
    }

    let handles = app_state.take_server_threads();

    if !handles.is_empty() {
        tauri::async_runtime::spawn_blocking(move || {
            // 并行 join 所有 worker，统一带总超时等待，避免极端情况下停止按钮永久挂起，
            // 也避免按 worker 串行等待导致总耗时 = 数量 × 单次超时
            let count = handles.len();
            let (tx, rx) = std::sync::mpsc::channel();
            for handle in handles {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let _ = handle.join();
                    let _ = tx.send(());
                });
            }
            drop(tx);
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_secs(SERVER_STOP_TIMEOUT_SECS);
            for _ in 0..count {
                let remain = deadline.saturating_duration_since(std::time::Instant::now());
                if rx.recv_timeout(remain).is_err() {
                    log::warn!("[共享] worker线程退出超时，跳过等待");
                    break;
                }
            }
            log::info!("[共享] 所有worker线程已退出");
        })
        .await
        .map_err(|e| {
            log::error!("[共享] 等待worker线程退出失败: {}", e);
            AppError::Other(format!("等待服务器线程退出失败: {}", e))
        })?;
    }

    app_state.set_server_stopped();

    log::info!("[共享] 服务器已停止");
    Ok(())
}

/// 查询当前共享状态
///
/// 前端在 webview 重载后调用：此时后端的服务器可能仍在运行，而前端已丢失
/// `isSharing`/`shareInfo` 状态，只能重启应用才能停止共享。本命令同时返回
/// 上次扫描的文件夹路径，便于前端一并恢复文件列表。
#[tauri::command]
pub fn get_share_status(state: State<'_, AppState>) -> ShareStatus {
    state.share_status()
}
