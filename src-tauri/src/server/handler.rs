use std::io::Read;

use crate::password;
use crate::SHARED_VIDEOS;

static LOGIN_PAGE: &str = include_str!("../login_template.html");

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
            let json = serde_json::to_string(&videos).unwrap_or_else(|_| "[]".to_string());
            let mut resp = super::response::json_response(200, &json);
            resp.add_header(
                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap(),
            );
            resp
        }
        "/refresh" => {
            let folder_path = crate::SHARED_FOLDER_PATH.read().clone();
            let result = if !folder_path.is_empty() {
                match crate::commands::video::scan_videos(folder_path) {
                    Ok(_) => serde_json::json!({"success": true, "message": "视频列表已刷新"}).to_string(),
                    Err(e) => serde_json::json!({"success": false, "message": e.to_string()}).to_string(),
                }
            } else {
                serde_json::json!({"success": false, "message": "未设置共享文件夹"}).to_string()
            };
            super::response::json_response(200, &result)
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
