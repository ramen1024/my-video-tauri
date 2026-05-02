mod handler;
mod response;
mod auth;
mod video_serve;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::SERVER_RUNNING;
use crate::password;

pub fn start_http_server(ips: &[String], port: u16) {
    let addr = format!("0.0.0.0:{}", port);
    let ips = ips.to_vec();

    match tiny_http::Server::http(&addr) {
        Ok(server) => {
            let server = Arc::new(server);
            crate::SERVER_HANDLE.write().replace(server.clone());

            let mut request_count: u64 = 0;
            for mut request in server.incoming_requests() {
                if !SERVER_RUNNING.load(Ordering::SeqCst) {
                    break;
                }

                request_count += 1;
                if request_count % 100 == 0 {
                    password::cleanup_expired_sessions();
                }

                let resp = handler::handle_request(&mut request, &ips, port);
                request.respond(resp).ok();
            }
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
        }
    }
}
