use std::io::{Cursor, Read};

pub fn text_response(status_code: u16, message: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let bytes = message.as_bytes().to_vec();
    let len = bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(bytes));
    tiny_http::Response::new(
        status_code.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
        ],
        cursor,
        Some(len),
        None,
    )
}

pub fn json_response(status_code: u16, json: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let bytes = json.as_bytes().to_vec();
    let len = bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(bytes));
    tiny_http::Response::new(
        status_code.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
            tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        ],
        cursor,
        Some(len),
        None,
    )
}

pub fn html_response(html: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let bytes = html.as_bytes().to_vec();
    let len = bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(bytes));
    tiny_http::Response::new(
        200.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
        ],
        cursor,
        Some(len),
        None,
    )
}

pub fn redirect_response(location: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let body = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><script>window.location.href='{}';</script></head><body></body></html>"#,
        location
    );
    let bytes = body.as_bytes().to_vec();
    let len = bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(bytes));
    tiny_http::Response::new(
        302.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Location"[..], location.as_bytes()).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], len.to_string().as_bytes()).unwrap(),
        ],
        cursor,
        Some(len),
        None,
    )
}
