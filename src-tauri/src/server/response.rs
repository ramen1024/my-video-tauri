//! HTTP 响应构造工具
//!
//! 提供构造 text/json/html/redirect/静态资源等 HTTP 响应的辅助函数，
//! 统一处理 Content-Type、Content-Length 等通用头部。

use std::io::{Cursor, Read};

use rand::RngCore;

/// 构造基础 HTTP 响应
///
/// 设置状态码、Content-Type、Content-Length 以及通用安全响应头
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
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes())
                .unwrap(),
            tiny_http::Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Referrer-Policy"[..], &b"no-referrer"[..]).unwrap(),
            // 禁止页面被嵌入 iframe，防止点击劫持
            tiny_http::Header::from_bytes(&b"X-Frame-Options"[..], &b"DENY"[..]).unwrap(),
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

/// 生成 CSP nonce（16 字节随机数的十六进制表示）
pub fn generate_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 构造前端页面（SPA 首页 / 登录页）响应
///
/// 为内联 `<script>` 注入 CSP nonce，使页面无需放行 `script-src 'unsafe-inline'`
/// 即可执行脚本（`'unsafe-inline'` 一旦放行，任何注入的脚本都会被执行）。
/// 页面内容随构建变化且携带 nonce，必须禁止缓存。
///
/// `style-src` 仍保留 `'unsafe-inline'`：Svelte 会在运行时注入 `<style>`，
/// 而样式注入无脚本执行能力，风险远低于脚本。
pub fn frontend_html_response(
    html: &str,
    nonce: &str,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    let html = html.replace("<script>", &format!("<script nonce=\"{}\">", nonce));
    let mut resp = build_response(200, "text/html; charset=utf-8", html.into_bytes());
    let csp = format!(
        "default-src 'self'; script-src 'self' 'nonce-{}'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; media-src 'self' blob:; connect-src 'self'; object-src 'none'; \
         base-uri 'none'; frame-ancestors 'none'",
        nonce
    );
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Content-Security-Policy"[..], csp.as_bytes()).unwrap(),
    );
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap(),
    );
    resp
}

/// 构造静态资源响应（前端 JS/CSS/图片等）
pub fn static_asset_response(
    bytes: Vec<u8>,
    mime_type: &str,
    cache_control: &str,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut resp = build_response(200, mime_type, bytes);
    resp.add_header(
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], cache_control.as_bytes()).unwrap(),
    );
    resp
}

/// 构造 416 Range Not Satisfiable 响应（带 Content-Range 提示实际文件大小）
pub fn range_not_satisfiable_response(file_size: u64) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut resp = text_response(416, "Range Not Satisfiable");
    resp.add_header(
        tiny_http::Header::from_bytes(
            &b"Content-Range"[..],
            format!("bytes */{}", file_size).as_bytes(),
        )
        .unwrap(),
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
    resp.add_header(tiny_http::Header::from_bytes(&b"Location"[..], location.as_bytes()).unwrap());
    resp
}
