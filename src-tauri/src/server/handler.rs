//! HTTP 请求路由与处理
//!
//! 根据请求 URL 和方法分发到对应的处理逻辑：
//! - `POST /auth`: 密码认证
//! - `GET /`: 首页（视频列表 HTML）
//! - `GET /videos`: 视频列表 JSON API
//! - `GET /refresh`: 触发视频重新扫描
//! - `GET /refresh-status`: 查询刷新结果
//! - `GET /video/*`: 视频文件流式服务
//! - 其他: 404
//!
//! 密码保护启用时，未认证请求会被重定向到登录页。

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::password;
use crate::SHARED_VIDEOS;

/// 登录页面 HTML 模板（编译时嵌入）
static LOGIN_PAGE: &str = include_str!("../login_template.html");
/// 刷新操作是否正在进行中（防止并发刷新）
static REFRESH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
/// 刷新冷却标志（5 秒内不允许再次刷新）
static REFRESH_COOLDOWN: AtomicBool = AtomicBool::new(false);

use parking_lot::RwLock;
use std::sync::LazyLock;

/// 最近一次刷新操作的结果（JSON 字符串）
static REFRESH_RESULT: LazyLock<RwLock<Option<String>>> =
    LazyLock::new(|| RwLock::new(None));

/// HTTP 请求主处理函数
///
/// 根据请求 URL 和方法路由到对应的处理逻辑。
/// 密码保护启用时，未认证请求会被重定向到 `/login`。
pub fn handle_request(
    request: &mut tiny_http::Request,
    ips: &[String],
    port: u16,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    let url = request.url().to_string();
    let method = request.method().clone();

    if url == "/auth" && method == tiny_http::Method::Post {
        return super::auth::handle_auth(request);
    }

    if password::is_password_enabled() {
        let cookie_header = request
            .headers()
            .iter()
            .find(|h| h.field.as_str() == "Cookie")
            .map(|h| h.value.as_str())
            .unwrap_or("");

        if !password::check_web_auth(cookie_header) {
            if url == "/login" || url == "/login.html" {
                return super::response::html_response(LOGIN_PAGE);
            }
            return super::response::redirect_response("/login");
        }
    }

    match url.as_str() {
        "/" | "/index.html" => {
            let addresses: String = ips
                .iter()
                .map(|ip| format!(r#"<span class="address-item">http://{}:{}</span>"#, ip, port))
                .collect::<Vec<_>>()
                .join(" | ");

            let template = include_str!("../html_template.html");
            let html = template.replace("{addresses}", &addresses);
            super::response::html_response(&html)
        }
        "/videos" => {
            let videos = SHARED_VIDEOS.read().clone();
            let json = serde_json::to_string(&*videos).unwrap_or_else(|_| "[]".to_string());
            let mut resp = super::response::json_response(200, &json);
            resp.add_header(
                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap(),
            );
            resp
        }
        "/refresh" => {
            if REFRESH_IN_PROGRESS.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                let json = serde_json::json!({"success": false, "message": "正在刷新中，请稍后"}).to_string();
                return super::response::json_response(429, &json);
            }
            if REFRESH_COOLDOWN.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
                REFRESH_IN_PROGRESS.store(false, Ordering::Release);
                let json = serde_json::json!({"success": false, "message": "刷新过于频繁，请稍后再试"}).to_string();
                return super::response::json_response(429, &json);
            }

            let folder_path = crate::SHARED_FOLDER_PATH.read().clone();
            if folder_path.is_empty() {
                REFRESH_IN_PROGRESS.store(false, Ordering::Release);
                REFRESH_COOLDOWN.store(false, Ordering::Release);
                let json = serde_json::json!({"success": false, "message": "未设置共享文件夹"}).to_string();
                return super::response::json_response(400, &json);
            }

            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match crate::commands::video::scan_videos_sync(folder_path) {
                        Ok(_) => serde_json::json!({"success": true, "message": "视频列表已刷新"}).to_string(),
                        Err(e) => serde_json::json!({"success": false, "message": e.to_string()}).to_string(),
                    }
                }));
                let msg = result.unwrap_or_else(|_| {
                    serde_json::json!({"success": false, "message": "刷新过程中发生内部错误"}).to_string()
                });
                log::info!("[刷新] {}", msg);
                let mut result_store = REFRESH_RESULT.write();
                *result_store = Some(msg);
                REFRESH_IN_PROGRESS.store(false, Ordering::Release);
            });

            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(5));
                REFRESH_COOLDOWN.store(false, Ordering::Release);
            });

            let json = serde_json::json!({"success": true, "message": "刷新已开始"}).to_string();
            super::response::json_response(202, &json)
        }
        "/refresh-status" => {
            let result = REFRESH_RESULT.read().clone();
            match result {
                Some(msg) => super::response::json_response(200, &msg),
                None => super::response::json_response(200, r#"{"success": true, "message": "无刷新记录"}"#),
            }
        }
        _ if url.starts_with("/video/") => {
            let range_header = request
                .headers()
                .iter()
                .find(|h| h.field.as_str() == "Range")
                .map(|h| h.value.as_str());
            super::video_serve::handle_video_request(&url, range_header)
        }
        _ => super::response::text_response(404, "Not found"),
    }
}
