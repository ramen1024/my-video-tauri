use std::io::Read;

use crate::password;

pub fn handle_auth(request: &mut tiny_http::Request) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut body = String::new();
    if let Some(len) = request.body_length() {
        let mut limited = request.as_reader().take(len as u64);
        let _ = limited.read_to_string(&mut body);
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
                    let json_str = json.to_string();
                    let bytes = json_str.as_bytes().to_vec();
                    let len = bytes.len();
                    let cursor: Box<dyn Read + Send> = Box::new(std::io::Cursor::new(bytes));
                    tiny_http::Response::new(
                        200.into(),
                        vec![
                            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
                            tiny_http::Header::from_bytes(
                                &b"Set-Cookie"[..],
                                format!("session_token={}; Path=/; Max-Age=3600; HttpOnly; SameSite=Strict", token).as_bytes(),
                            ).unwrap(),
                        ],
                        cursor,
                        Some(len),
                        None,
                    )
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
