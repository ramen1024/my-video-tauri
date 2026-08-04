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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::constants::{
    MAX_CONCURRENT_STREAMS, SERVER_WORKER_DEFAULT_COUNT, SERVER_WORKER_MAX_COUNT,
};
use crate::AppState;

/// 简单计数信号量：限制视频流式响应并发线程数
struct StreamLimiter {
    available: AtomicUsize,
}

impl StreamLimiter {
    fn new(max: usize) -> Self {
        Self {
            available: AtomicUsize::new(max),
        }
    }

    /// 尝试获取一个许可，成功时返回 RAII 许可
    fn try_acquire(self: &Arc<Self>) -> Option<StreamPermit> {
        let acquired = self
            .available
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n > 0).then_some(n - 1)
            })
            .is_ok();
        acquired.then(|| StreamPermit(self.clone()))
    }
}

/// RAII 信号量许可：作用域结束时自动归还计数
struct StreamPermit(Arc<StreamLimiter>);

impl Drop for StreamPermit {
    fn drop(&mut self) {
        self.0.available.fetch_add(1, Ordering::Release);
    }
}

/// 启动 HTTP 服务器
///
/// 绑定 `0.0.0.0:port` 监听所有网络接口，创建多 worker 线程处理请求。
/// 每个 worker 循环接收请求并交给 handler 处理，直到服务器状态变为非 Running。
/// 视频流式响应在独立线程中写出，worker 只处理小请求，因此线程数设上限。
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
        .unwrap_or(SERVER_WORKER_DEFAULT_COUNT)
        .min(SERVER_WORKER_MAX_COUNT);

    log::info!("[HTTP服务器] 启动: addr={}, workers={}", addr, worker_count);

    // 视频流式响应并发上限：超出时在 worker 内同步写出形成背压，
    // 避免每路视频流各开一个线程导致线程数无限膨胀
    let stream_limiter = Arc::new(StreamLimiter::new(MAX_CONCURRENT_STREAMS));

    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let server = server.clone();
        let ips = ips.clone();
        let app_state = app_state.clone();
        let stream_limiter = stream_limiter.clone();
        let handle = std::thread::spawn(move || {
            loop {
                // 服务器停止（含 Stopping 状态）时退出 worker 循环，
                // 阻塞中的 recv 由调用方 unblock() 唤醒
                if !app_state.is_server_running() {
                    break;
                }

                match server.incoming_requests().next() {
                    Some(mut request) => {
                        let resp = handler::handle_request(&mut request, &ips, port, &app_state);
                        let url = request.url().to_string();
                        if url.starts_with("/video/") {
                            // 视频流式响应可能长时间占用连接，放到独立线程写出，
                            // 避免阻塞 worker 循环，影响 /videos、/refresh、/auth 等请求。
                            match stream_limiter.try_acquire() {
                                Some(permit) => {
                                    std::thread::spawn(move || {
                                        let _ = request.respond(resp);
                                        drop(permit);
                                    });
                                }
                                None => {
                                    // 并发流已达上限：退回在 worker 内同步写出（背压）
                                    request.respond(resp).ok();
                                }
                            }
                        } else {
                            request.respond(resp).ok();
                        }
                    }
                    None => break,
                }
            }
        });
        handles.push(handle);
    }

    Ok((server, handles))
}
