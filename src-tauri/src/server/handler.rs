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

use sha2::{Digest, Sha256};

use crate::constants::REFRESH_COOLDOWN_SECS;
use crate::models::VideoFile;
use crate::password;
use crate::AppState;

/// 登录页面 HTML 模板（编译时嵌入）
static LOGIN_PAGE: &str = include_str!("../login_template.html");

/// 为视频列表计算 ETag
///
/// 基于视频数量、总大小、第一个和最后一个视频的关键字段生成一个稳定标识。
/// 使用 SHA-256 替代默认哈希，保证跨进程、跨平台结果一致。
fn compute_videos_etag(videos: &[VideoFile]) -> String {
    let count = videos.len();
    let total_size: u64 = videos.iter().map(|v| v.size).sum();
    let first = videos.first().map(|v| (v.relative_path.clone(), v.size));
    let last = videos.last().map(|v| (v.relative_path.clone(), v.size));

    let mut hasher = Sha256::new();
    hasher.update(&count.to_be_bytes());
    hasher.update(&total_size.to_be_bytes());
    if let Some((path, size)) = &first {
        hasher.update(path.as_bytes());
        hasher.update(&size.to_be_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some((path, size)) = &last {
        hasher.update(path.as_bytes());
        hasher.update(&size.to_be_bytes());
    } else {
        hasher.update(&[0]);
    }

    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// HTTP 请求主处理函数
///
/// 根据请求 URL 和方法路由到对应的处理逻辑。
/// 密码保护启用时，未认证请求会被重定向到 `/login`。
pub fn handle_request(
    request: &mut tiny_http::Request,
    ips: &[String],
    port: u16,
    app_state: &AppState,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    // Host 头校验，防止 DNS rebinding 攻击：
    // 仅允许本机 IP（含回环地址）作为 Host，恶意网页无法通过域名解析指向本机后"同源"读取 /videos。
    let host_allowed = request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("Host"))
        .map(|h| h.value.as_str().split(':').next().unwrap_or(""))
        .map(|host| {
            ips.iter().any(|ip| ip == host)
                || host == "127.0.0.1"
                || host == "localhost"
                || host == "::1"
        })
        .unwrap_or(false);
    if !host_allowed {
        return super::response::text_response(403, "Invalid Host header");
    }

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
            let videos = app_state.shared_videos();
            let etag = format!("\"{}\"", compute_videos_etag(&videos));

            let if_none_match = request
                .headers()
                .iter()
                .find(|h| h.field.as_str() == "If-None-Match")
                .map(|h| h.value.as_str());

            if if_none_match == Some(etag.as_str()) {
                let mut resp = super::response::text_response(304, "");
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"ETag"[..], etag.as_bytes()).unwrap(),
                );
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"must-revalidate"[..])
                        .unwrap(),
                );
                return resp;
            }

            let json = serde_json::to_string(&*videos).unwrap_or_else(|_| "[]".to_string());
            let mut resp = super::response::json_response(200, &json);
            resp.add_header(
                tiny_http::Header::from_bytes(&b"ETag"[..], etag.as_bytes()).unwrap(),
            );
            resp.add_header(
                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"must-revalidate"[..])
                    .unwrap(),
            );
            resp
        }
        "/refresh" => {
            if !app_state.start_refresh() {
                let json = serde_json::json!({"success": false, "message": "正在刷新中，请稍后"})
                    .to_string();
                return super::response::json_response(429, &json);
            }
            if !app_state.start_refresh_cooldown() {
                app_state.finish_refresh();
                let json =
                    serde_json::json!({"success": false, "message": "刷新过于频繁，请稍后再试"})
                        .to_string();
                return super::response::json_response(429, &json);
            }

            let folder_path = app_state.shared_folder_path();
            if folder_path.is_empty() {
                app_state.finish_refresh();
                app_state.finish_refresh_cooldown();
                let json =
                    serde_json::json!({"success": false, "message": "未设置共享文件夹"}).to_string();
                return super::response::json_response(400, &json);
            }

            // 清空上次刷新结果，使 /refresh-status 能准确反映本次刷新状态
            app_state.clear_refresh_result();

            let scan_app_state = app_state.clone();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match crate::commands::video::scan_videos_sync(
                        folder_path,
                        &scan_app_state,
                        false,
                    ) {
                        Ok(_) => {
                            serde_json::json!({"success": true, "message": "视频列表已刷新"})
                                .to_string()
                        }
                        Err(e) => {
                            serde_json::json!({"success": false, "message": e.to_string()})
                                .to_string()
                        }
                    }
                }));
                let msg = result.unwrap_or_else(|_| {
                    serde_json::json!({"success": false, "message": "刷新过程中发生内部错误"})
                        .to_string()
                });
                log::info!("[刷新] {}", msg);
                scan_app_state.set_refresh_result(msg);
                scan_app_state.finish_refresh();
            });

            let cooldown_app_state = app_state.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(REFRESH_COOLDOWN_SECS));
                cooldown_app_state.finish_refresh_cooldown();
            });

            let json = serde_json::json!({"success": true, "message": "刷新已开始"}).to_string();
            super::response::json_response(202, &json)
        }
        "/refresh-status" => {
            let result = app_state.refresh_result();
            match result {
                Some(msg) => super::response::json_response(200, &msg),
                None => super::response::json_response(
                    200,
                    r#"{"success": true, "message": "无刷新记录"}"#,
                ),
            }
        }
        _ if url.starts_with("/video/") => {
            let range_header = request
                .headers()
                .iter()
                .find(|h| h.field.as_str() == "Range")
                .map(|h| h.value.as_str());
            super::video_serve::handle_video_request(&url, range_header, app_state)
        }
        _ => super::response::text_response(404, "Not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::compute_videos_etag;
    use crate::models::VideoFile;

    fn sample_video(name: &str, relative_path: &str, size: u64) -> VideoFile {
        VideoFile {
            name: name.to_string(),
            path: format!("/tmp/{}", name),
            relative_path: relative_path.to_string(),
            size,
            modified: None,
            extension: "mp4".to_string(),
        }
    }

    #[test]
    fn test_compute_videos_etag_is_stable() {
        let videos = vec![
            sample_video("a.mp4", "a.mp4", 1024),
            sample_video("b.mp4", "b.mp4", 2048),
        ];
        let etag1 = compute_videos_etag(&videos);
        let etag2 = compute_videos_etag(&videos);
        assert_eq!(etag1, etag2, "同一视频列表生成的 ETag 应保持一致");
        assert_eq!(
            etag1.len(),
            64,
            "SHA-256 十六进制输出长度应为 64"
        );
    }

    #[test]
    fn test_compute_videos_etag_differs_for_different_lists() {
        let videos_a = vec![sample_video("a.mp4", "a.mp4", 1024)];
        let videos_b = vec![sample_video("a.mp4", "a.mp4", 2048)];
        assert_ne!(
            compute_videos_etag(&videos_a),
            compute_videos_etag(&videos_b),
            "不同视频列表应生成不同 ETag"
        );
    }
}
