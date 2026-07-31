//! HTTP 响应构造工具
//!
//! 提供构造 text/json/html/redirect 等 HTTP 响应的辅助函数，
//! 统一处理 Content-Type、Content-Length 等通用头部。

use std::io::{Cursor, Read};

/// 构造基础 HTTP 响应
///
/// 设置状态码、Content-Type 和 Content-Length 头部
fn build_response(
    status_code: u16,
    content_type: &str,
    body: Vec<u8>,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    let len = body.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(body));
    tiny_http::Response::new(
        status_code.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
            tiny_http::Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..])
                .unwrap(),
            tiny_http::Header::from_bytes(&b"Referrer-Policy"[..], &b"no-referrer"[..]).unwrap(),
        ],
        cursor,
        Some(len),
        None,
    )
}

/// 构造纯文本响应
pub fn text_response(status_code: u16, message: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    build_response(status_code, "text/plain", message.as_bytes().to_vec())
}

/// 构造 JSON 响应
pub fn json_response(status_code: u16, json: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    build_response(status_code, "application/json", json.as_bytes().to_vec())
}

/// 构造 HTML 响应，附带 Content-Security-Policy 头部
pub fn html_response(html: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut resp = build_response(200, "text/html; charset=utf-8", html.as_bytes().to_vec());
    resp.add_header(
        tiny_http::Header::from_bytes(
            &b"Content-Security-Policy"[..],
            b"default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; img-src 'self' data:; media-src 'self' blob:; connect-src 'self'",
        ).unwrap(),
    );
    resp
}

/// 构造 302 重定向响应（同时包含 JS 跳转作为后备）
pub fn redirect_response(location: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let body = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><script>window.location.href='{}';</script></head><body></body></html>"#,
        location
    );
    let mut resp = build_response(302, "text/html; charset=utf-8", body.as_bytes().to_vec());
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Location"[..], location.as_bytes()).unwrap(),
    );
    resp
}
