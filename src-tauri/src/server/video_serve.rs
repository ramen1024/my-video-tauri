//! 视频文件流式服务
//!
//! 处理 `/video/*` 请求，支持 HTTP Range 请求（部分内容），
//! 使浏览器可以拖动视频进度条进行 seek 操作。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::utils::sanitize_video_path;
use crate::AppState;

/// 处理视频文件请求
///
/// 解析 URL 中的视频路径，验证安全性后返回文件内容。
/// 支持 Range 请求头，返回 206 Partial Content 响应。
pub fn handle_video_request(
    url: &str,
    range_header: Option<&str>,
    app_state: &AppState,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    let video_name = url.strip_prefix("/video/").unwrap_or("");
    if video_name.is_empty() {
        return super::response::text_response(400, "Invalid video path");
    }

    let folder_path = app_state.shared_folder_path();

    let video_path = match sanitize_video_path(Path::new(&folder_path), video_name) {
        Some(path) => path,
        None => {
            return super::response::text_response(403, "Access denied: invalid path");
        }
    };

    if !video_path.exists() {
        return super::response::text_response(404, "File not found");
    }

    let file = match File::open(&video_path) {
        Ok(f) => f,
        Err(_) => return super::response::text_response(404, "File not found"),
    };

    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let mut range_start = 0u64;
    let mut range_end = file_size.saturating_sub(1);
    let mut has_range = false;

    if let Some(range_value) = range_header {
        if range_value.starts_with("bytes=") {
            let parts: Vec<&str> = range_value[6..].split('-').collect();
            if let Some(start) = parts.first() {
                if !start.is_empty() {
                    range_start = start.parse().unwrap_or(0);
                }
            }
            if let Some(end) = parts.get(1) {
                if !end.is_empty() {
                    range_end = end.parse().unwrap_or(file_size.saturating_sub(1));
                }
            }

            if range_start <= range_end && range_start < file_size {
                has_range = true;
            } else {
                range_start = 0;
                range_end = file_size.saturating_sub(1);
            }
        }
    }

    let content_length = range_end.saturating_sub(range_start) + 1;

    let mut file = file;
    if file.seek(SeekFrom::Start(range_start)).is_err() {
        return super::response::text_response(500, "Seek error");
    }

    let content_type = match video_path.extension().and_then(|e| e.to_str()) {
        Some("mp4" | "m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        Some("mov") => "video/quicktime",
        Some("wmv") => "video/x-ms-wmv",
        Some("flv") => "video/x-flv",
        Some("mpg" | "mpeg") => "video/mpeg",
        _ => "application/octet-stream",
    };

    let limited_reader = file.take(content_length);
    let boxed_reader: Box<dyn Read + Send> = Box::new(limited_reader);

    let mut headers = vec![
        tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
        tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
        tiny_http::Header::from_bytes(
            &b"Content-Length"[..],
            content_length.to_string().as_bytes(),
        )
        .unwrap(),
        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
    ];

    if has_range {
        headers.push(
            tiny_http::Header::from_bytes(
                &b"Content-Range"[..],
                format!("bytes {}-{}/{}", range_start, range_end, file_size).as_bytes(),
            )
            .unwrap(),
        );
    }

    tiny_http::Response::new(
        if has_range { 206 } else { 200 }.into(),
        headers,
        boxed_reader,
        Some(content_length as usize),
        None,
    )
}
