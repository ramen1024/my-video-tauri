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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::constants::{
    MAX_CONCURRENT_STREAMS, SERVER_WORKER_DEFAULT_COUNT, SERVER_WORKER_MAX_COUNT,
};
use crate::AppState;

/// 单次 HTTP 服务器实例的停止信号
///
/// worker 的存活**只**由本信号决定，不再读取 `AppState::ServerState`：服务器从
/// "端口已绑定、worker 已启动"到"状态切换为 Running"之间存在窗口（`start_http_server`
/// 返回后才由调用方 `set_server_running`），若 worker 用状态机判断自己是否该工作，
/// 就会在这段窗口内把 `Starting` 误判为"已停止"而全部退出——端口仍在监听，却没有任何
/// 线程去取请求，客户端只能一直挂起。
///
/// 每次启动都会新建一个信号，因此停止旧实例不会影响重启后的新实例。
#[derive(Clone, Default)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    /// 创建未触发的停止信号
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// 请求停止：worker 在下一次循环判断或 `unblock` 唤醒后退出
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// 是否已请求停止
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// 一次 HTTP 服务器实例的运行句柄
///
/// 把监听套接字与本次实例的停止信号绑在一起：二者生命周期天然一致，
/// 由 `AppState::set_server_stopped` 清空句柄时一并释放。
pub struct RunningServer {
    server: Arc<tiny_http::Server>,
    stop_signal: StopSignal,
}

impl RunningServer {
    /// 监听套接字
    ///
    /// 仅测试使用：读取实际绑定的端口（生产路径传入明确端口，无需回读）。
    #[cfg(test)]
    pub fn server(&self) -> &Arc<tiny_http::Server> {
        &self.server
    }

    /// 优雅停止：先置停止信号，再逐个唤醒阻塞在 `recv` 的 worker
    ///
    /// `worker_count` 为当前 worker 数量：`unblock()` 每次只唤醒一个阻塞中的 worker，
    /// 且以"哨兵"形式入队（没有线程在等也不会丢失）。
    pub fn shutdown(&self, worker_count: usize) {
        self.stop_signal.stop();
        for _ in 0..worker_count.max(1) {
            self.server.unblock();
        }
    }
}

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
/// worker 循环接收请求并交给 handler 处理，直到 `stop_signal` 被触发。
/// 视频流式响应在独立线程中写出，worker 只处理小请求，因此线程数设上限。
///
/// `stop_signal` 由调用方在停止时触发，与 `AppState` 的服务器状态机解耦，
/// 详见 [`StopSignal`] 的说明。
///
/// # 返回
/// - `Ok((running, handles))`: 运行句柄与 worker 线程句柄
/// - `Err(msg)`: 绑定端口失败时的错误信息
pub fn start_http_server(
    ips: &[String],
    port: u16,
    app_state: Arc<AppState>,
    stop_signal: StopSignal,
) -> Result<(RunningServer, Vec<std::thread::JoinHandle<()>>), String> {
    let addr = format!("0.0.0.0:{}", port);
    let ips = ips.to_vec();

    let server = tiny_http::Server::http(&addr).map_err(|e| format!("启动服务器失败: {}", e))?;

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
        let stop_signal = stop_signal.clone();
        let handle = std::thread::spawn(move || {
            loop {
                // 退出条件只看本实例的停止信号：状态机在启动完成前是 Starting，
                // 用它判断会让 worker 在启动窗口内集体自杀（见 StopSignal 说明）
                if stop_signal.is_stopped() {
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

    Ok((
        RunningServer {
            server,
            stop_signal,
        },
        handles,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    /// 测试用 Host：`handle_request` 只放行本机 IP / localhost
    const TEST_IP: &str = "127.0.0.1";

    /// 向测试服务器发一个最小 HTTP/1.1 请求并读回完整响应
    fn http_get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect((TEST_IP, port)).expect("连接测试服务器失败");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("设置读超时失败");
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, TEST_IP
        );
        stream.write_all(request.as_bytes()).expect("写入请求失败");

        let mut response = String::new();
        stream.read_to_string(&mut response).expect("读取响应失败");
        response
    }

    /// 读取响应状态行，例如 "HTTP/1.1 200 OK"
    fn status_line(response: &str) -> &str {
        response.lines().next().unwrap_or("")
    }

    /// 在超时内并行等待所有 worker 退出，返回成功退出的线程数
    ///
    /// 与 `stop_share_server` 的做法一致：不用无超时的 `join`，
    /// 否则实现出错时测试会永久挂起而不是失败。
    fn join_all_with_timeout(
        handles: Vec<std::thread::JoinHandle<()>>,
        timeout: Duration,
    ) -> usize {
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

        let deadline = Instant::now() + timeout;
        let mut finished = 0;
        for _ in 0..count {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if rx.recv_timeout(remaining).is_err() {
                break;
            }
            finished += 1;
        }
        finished
    }

    /// 启动一个绑定到临时端口的测试服务器，返回 (运行句柄, worker 句柄, 端口)
    fn start_test_server(
        app_state: &Arc<AppState>,
    ) -> (RunningServer, Vec<std::thread::JoinHandle<()>>, u16) {
        // 传 0 让操作系统分配空闲端口，测试之间不会互相抢端口
        let (running, handles) = start_http_server(
            &[TEST_IP.to_string()],
            0,
            app_state.clone(),
            StopSignal::new(),
        )
        .expect("测试服务器应能启动");
        let port = running
            .server()
            .server_addr()
            .to_ip()
            .expect("应绑定到 IP 地址")
            .port();
        (running, handles, port)
    }

    /// P0 回归测试：状态仍是 Starting 时，worker 必须已经在服务请求
    ///
    /// 旧实现让 worker 循环首行判断 `AppState::is_server_running()`，而状态要等
    /// `start_http_server` 返回、调用方执行 `set_server_running` 之后才变为 Running。
    /// worker 在启动窗口内看到的必然是 Starting，于是全部立刻退出：端口仍在监听，
    /// 却没有任何线程去取请求，客户端只能一直挂起（"开了共享但页面打不开"）。
    ///
    /// 本测试故意停在 Starting 状态发起真实 HTTP 请求，因此对旧实现是必然失败，
    /// 而与本机核数、调度时序无关。
    #[test]
    fn workers_serve_requests_while_state_is_starting() {
        let app_state = Arc::new(AppState::new());
        app_state
            .start_server_starting()
            .expect("应能从 Stopped 进入 Starting");
        assert_eq!(
            app_state.server_state(),
            crate::ServerState::Starting,
            "前置条件：状态必须停在 Starting"
        );

        let (running, handles, port) = start_test_server(&app_state);
        let worker_count = handles.len();
        let response = http_get(port, "/videos");

        assert!(
            status_line(&response).contains("200"),
            "Starting 状态下 worker 必须仍在服务请求，实际状态行: {:?}",
            status_line(&response)
        );

        running.shutdown(worker_count);
        assert_eq!(
            join_all_with_timeout(handles, Duration::from_secs(5)),
            worker_count,
            "测试结束后应能干净地停掉所有 worker"
        );
    }

    /// 停止信号必须能让所有阻塞在 recv 的 worker 退出
    #[test]
    fn stop_signal_terminates_all_workers() {
        let app_state = Arc::new(AppState::new());
        let (running, handles, _port) = start_test_server(&app_state);
        let worker_count = handles.len();
        assert!(worker_count > 0, "应当创建至少一个 worker");

        running.shutdown(worker_count);

        assert_eq!(
            join_all_with_timeout(handles, Duration::from_secs(5)),
            worker_count,
            "停止信号 + unblock 后所有 worker 都应在超时内退出"
        );
    }

    /// 重启后的新实例只受自己的停止信号影响
    ///
    /// 旧实例的停止信号若被复用，会让新实例的 worker 一启动就退出。
    #[test]
    fn new_instance_is_not_affected_by_previous_stop_signal() {
        let app_state = Arc::new(AppState::new());

        app_state.start_server_starting().expect("首次启动");
        let (first, first_handles, _) = start_test_server(&app_state);
        let first_count = first_handles.len();
        first.shutdown(first_count);
        assert_eq!(
            join_all_with_timeout(first_handles, Duration::from_secs(5)),
            first_count,
            "旧实例的 worker 应全部退出"
        );

        // 模拟"停止 -> 再次启动"：状态回到 Stopped 后重新进入 Starting
        app_state.set_server_stopped();
        app_state.start_server_starting().expect("重新启动");

        let (second, second_handles, port) = start_test_server(&app_state);
        let second_count = second_handles.len();
        let response = http_get(port, "/videos");
        assert!(
            status_line(&response).contains("200"),
            "重启后的新实例必须正常服务，实际状态行: {:?}",
            status_line(&response)
        );

        second.shutdown(second_count);
        assert_eq!(
            join_all_with_timeout(second_handles, Duration::from_secs(5)),
            second_count,
            "新实例的 worker 也应能在停止信号后全部退出"
        );
    }

    /// 停止信号一旦触发即保持触发状态，可被多次查询
    #[test]
    fn stop_signal_is_sticky() {
        let signal = StopSignal::new();
        assert!(!signal.is_stopped(), "初始状态不应是已停止");
        signal.stop();
        assert!(signal.is_stopped(), "触发后应为已停止");
        signal.stop();
        assert!(signal.is_stopped(), "重复触发不应改变结果");
    }

    /// 关闭的端口必须能被重新绑定：停止后再次启动同一端口应当成功
    #[test]
    fn port_is_released_after_workers_exit() {
        let app_state = Arc::new(AppState::new());
        let (running, handles, port) = start_test_server(&app_state);
        let worker_count = handles.len();
        running.shutdown(worker_count);
        assert_eq!(
            join_all_with_timeout(handles, Duration::from_secs(5)),
            worker_count,
            "worker 应全部退出"
        );
        drop(running);

        // 端口已释放：直接重新绑定应当成功
        let rebound = tiny_http::Server::http(("127.0.0.1", port));
        assert!(rebound.is_ok(), "worker 退出后端口 {} 应可被重新绑定", port);
    }
}
