mod handler;
mod response;
mod auth;
mod video_serve;

use std::sync::Arc;

use crate::{SERVER_STATE, ServerState};

pub fn start_http_server(ips: &[String], port: u16) -> Result<Vec<std::thread::JoinHandle<()>>, String> {
    let addr = format!("0.0.0.0:{}", port);
    let ips = ips.to_vec();

    let server = tiny_http::Server::http(&addr)
        .map_err(|e| format!("启动服务器失败: {}", e))?;

    let server = Arc::new(server);
    crate::SERVER_HANDLE.write().replace(server.clone());

    let worker_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let server = server.clone();
        let ips = ips.clone();
        let handle = std::thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let running = {
                    let state = SERVER_STATE.lock();
                    matches!(*state, ServerState::Running)
                };
                if !running {
                    break;
                }
                let resp = handler::handle_request(&mut request, &ips, port);
                request.respond(resp).ok();
            }
        });
        handles.push(handle);
    }

    Ok(handles)
}
