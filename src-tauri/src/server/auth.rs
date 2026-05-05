//! HTTP 密码认证处理
//!
//! 处理 `POST /auth` 请求，验证密码并设置 session cookie。

use std::io::Read;

use crate::password;

/// 请求体最大允许大小（字节），防止恶意大请求
const MAX_BODY_SIZE: u64 = 1024;

/// 处理密码认证请求
///
/// 从请求体中解析密码，调用 password 模块进行认证。
/// 认证成功时设置 HttpOnly + SameSite=Strict 的 session cookie。
pub fn handle_auth(request: &mut tiny_http::Request) -> tiny_http::Response<Box<dyn Read + Send>> {
    password::cleanup_expired_sessions();

    let content_length = request.body_length().unwrap_or(0) as u64;

    if content_length > MAX_BODY_SIZE {
        return super::response::json_response(
            413,
            r#"{"success": false, "message": "请求体过大"}"#,
        );
    }

    let mut body = String::new();
    let mut limited = request.as_reader().take(content_length);
    if limited.read_to_string(&mut body).is_err() {
        return super::response::json_response(
            400,
            r#"{"success": false, "message": "读取请求体失败"}"#,
        );
    }

    let ip = request.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();

    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(data) => {
            let pwd = data["password"].as_str().unwrap_or("");
            if pwd.is_empty() {
                return super::response::json_response(400, r#"{"success": false, "message": "请输入密码"}"#);
            }

            match password::authenticate_web_request(&ip, pwd) {
                Ok(token) => {
                    let json = serde_json::json!({"success": true, "token": token});
                    let mut resp = super::response::json_response(200, &json.to_string());
                    resp.add_header(
                        tiny_http::Header::from_bytes(
                            &b"Set-Cookie"[..],
                            format!("session_token={}; Path=/; Max-Age=3600; HttpOnly; SameSite=Strict", token).as_bytes(),
                        ).unwrap(),
                    );
                    resp
                }
                Err(e) => {
                    let json = serde_json::json!({"success": false, "message": e});
                    super::response::json_response(401, &json.to_string())
                }
            }
        }
        Err(_) => {
            super::response::json_response(400, r#"{"success": false, "message": "无效的请求数据"}"#)
        }
    }
}
