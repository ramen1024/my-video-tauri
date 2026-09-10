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

use crate::constants::REFRESH_COOLDOWN_SECS;
use crate::models::VideoSummary;
use crate::AppState;

/// 登录页面 HTML 模板（编译时嵌入）
static LOGIN_PAGE: &str = include_str!("../login_template.html");

/// 按名称读取请求头值，字段名大小写不敏感（HTTP 头名规范要求）
///
/// 找不到时返回 `None`；调用方需要默认空串时可自行 `unwrap_or("")`。
fn request_header<'a>(request: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

/// 从 Host 头中取出主机名，丢弃端口部分
///
/// IPv6 字面量形如 `[::1]:6008`，方括号内本身含冒号，不能直接按 `:` 切分
/// （否则会得到 `[`，导致 IPv6 访问被误判为非法 Host）。
fn host_without_port(header: &str) -> &str {
    let host = header.trim();
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    host.split(':').next().unwrap_or(host)
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
    let host_allowed = request_header(request, "Host")
        .map(host_without_port)
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
        return super::auth::handle_auth(request, app_state);
    }

    if app_state.password().is_enabled() {
        let cookie_header = request_header(request, "Cookie").unwrap_or("");

        if !app_state.password().check_web_auth(cookie_header) {
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
                .map(|ip| {
                    format!(
                        r#"<span class="address-item">http://{}:{}</span>"#,
                        ip, port
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");

            let template = include_str!("../html_template.html");
            let html = template.replace("{addresses}", &addresses);
            super::response::html_response(&html)
        }
        "/videos" => {
            let etag = format!("\"{}\"", app_state.videos_etag());

            let if_none_match = request_header(request, "If-None-Match");

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

            let videos = app_state.shared_videos();
            // 只返回摘要：VideoFile.path 是本机绝对路径，不应泄露给局域网客户端
            let summaries: Vec<VideoSummary> = videos.iter().map(VideoSummary::from).collect();
            let json = serde_json::to_string(&summaries).unwrap_or_else(|_| "[]".to_string());
            let mut resp = super::response::json_response(200, &json);
            resp.add_header(tiny_http::Header::from_bytes(&b"ETag"[..], etag.as_bytes()).unwrap());
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
                let json = serde_json::json!({"success": false, "message": "未设置共享文件夹"})
                    .to_string();
                return super::response::json_response(400, &json);
            }

            // 清空上次刷新结果，使 /refresh-status 能准确反映本次刷新状态
            app_state.clear_refresh_result();

            let scan_app_state = app_state.clone();
            std::thread::spawn(move || {
                // 扫描路径不使用 unwrap / 索引，也没有 panic 源；且 release 构建为
                // panic = "abort"，catch_unwind 在此场景下本就无法生效，故不做包装
                let msg =
                    match crate::commands::video::scan_videos_sync(folder_path, &scan_app_state) {
                        Ok(_) => serde_json::json!({"success": true, "message": "视频列表已刷新"})
                            .to_string(),
                        Err(e) => serde_json::json!({"success": false, "message": e.to_string()})
                            .to_string(),
                    };
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
                // pending 字段是前端判断"是否已有结果"的依据，
                // 避免前后端靠 message 文案耦合（改文案会静默改变轮询行为）
                None => super::response::json_response(
                    200,
                    r#"{"success": true, "pending": true, "message": "无刷新记录"}"#,
                ),
            }
        }
        _ if url.starts_with("/video/") => {
            let range_header = request_header(request, "Range");
            super::video_serve::handle_video_request(&url, range_header, app_state)
        }
        _ => super::response::text_response(404, "Not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::host_without_port;

    #[test]
    fn test_host_without_port_strips_port() {
        assert_eq!(host_without_port("192.168.1.5:6008"), "192.168.1.5");
        assert_eq!(host_without_port("localhost:6008"), "localhost");
        assert_eq!(host_without_port("localhost"), "localhost");
        assert_eq!(host_without_port(" 192.168.1.5:6008 "), "192.168.1.5");
    }

    #[test]
    fn test_host_without_port_handles_ipv6_literal() {
        assert_eq!(host_without_port("[::1]:6008"), "::1");
        assert_eq!(host_without_port("[::1]"), "::1");
        assert_eq!(host_without_port("[fe80::1]:6008"), "fe80::1");
    }
}
