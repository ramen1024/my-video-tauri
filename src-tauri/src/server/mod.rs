//! HTTP 共享服务器模块
//!
//! 基于 tiny_http 实现的轻量级 HTTP 服务器，用于局域网视频共享。
//! 采用多 worker 线程模型，worker 数量等于 CPU 核心数。
//!
//! 子模块职责：
//! - `handler`: 请求路由与处理
//! - `auth`: 密码认证接口
//! - `video_serve`: 视频文件服务（支持 Range 请求）
//! - `response`: HTTP 响应构造工具

mod auth;
mod handler;
mod response;
mod video_serve;

use std::sync::Arc;

use crate::constants::SERVER_WORKER_DEFAULT_COUNT;
use crate::{AppState, ServerState};

/// 启动 HTTP 服务器
///
/// 绑定 `0.0.0.0:port` 监听所有网络接口，创建多 worker 线程处理请求。
/// 每个 worker 循环接收请求并交给 handler 处理，直到服务器状态变为非 Running。
///
/// # 返回
/// - `Ok((server, handles))`: 服务器实例和 worker 线程句柄
/// - `Err(msg)`: 绑定端口失败时的错误信息
pub fn start_http_server(
    ips: &[String],
    port: u16,
    app_state: Arc<AppState>,
) -> Result<(Arc<tiny_http::Server>, Vec<std::thread::JoinHandle<()>>), String> {
    let addr = format!("0.0.0.0:{}", port);
    let ips = ips.to_vec();

    let server = tiny_http::Server::http(&addr)
        .map_err(|e| format!("启动服务器失败: {}", e))?;

    let server = Arc::new(server);

    let worker_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(SERVER_WORKER_DEFAULT_COUNT);

    log::info!("[HTTP服务器] 启动: addr={}, workers={}", addr, worker_count);

    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let server = server.clone();
        let ips = ips.clone();
        let app_state = app_state.clone();
        let handle = std::thread::spawn(move || {
            loop {
                if !app_state.is_server_running() {
                    break;
                }

                match server.incoming_requests().next() {
                    Some(mut request) => {
                        let should_handle = {
                            let state = app_state.server_state();
                            matches!(state, ServerState::Running | ServerState::Stopping)
                        };
                        if !should_handle {
                            break;
                        }
                        let resp = handler::handle_request(&mut request, &ips, port, &app_state);
                        request.respond(resp).ok();
                    }
                    None => break,
                }
            }
        });
        handles.push(handle);
    }

    Ok((server, handles))
}
