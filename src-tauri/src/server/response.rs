use std::io::{Cursor, Read};

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
        ],
        cursor,
        Some(len),
        None,
    )
}

pub fn text_response(status_code: u16, message: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    build_response(status_code, "text/plain", message.as_bytes().to_vec())
}

pub fn json_response(status_code: u16, json: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    build_response(status_code, "application/json", json.as_bytes().to_vec())
}

pub fn html_response(html: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    build_response(200, "text/html; charset=utf-8", html.as_bytes().to_vec())
}

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
