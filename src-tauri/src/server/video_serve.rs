//! 视频文件流式服务
//!
//! 处理 `/video/*` 请求，支持 HTTP Range 请求（部分内容），
//! 使浏览器可以拖动视频进度条进行 seek 操作。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::constants::{is_supported_video_extension, video_content_type};
use crate::utils::sanitize_video_path;
use crate::AppState;

/// 解析 HTTP Range 请求头，返回满足的字节区间 `(start, end)`（闭区间）
///
/// 支持以下形式：
/// - `bytes=0-499`：指定区间
/// - `bytes=100-`：从 100 到文件末尾
/// - `bytes=-500`：最后 500 字节（后缀范围）
///
/// 返回 `None` 表示：无 Range 头、非 bytes 单位、或区间无法满足
/// （起始超出文件大小、起始大于结束、空文件、多段范围等）。
/// 无法满足的情况由调用方返回 416。
fn parse_range(range_header: Option<&str>, file_size: u64) -> Option<(u64, u64)> {
    let range_value = range_header?;
    if !range_value.starts_with("bytes=") {
        return None;
    }
    if file_size == 0 {
        return None;
    }

    let parts: Vec<&str> = range_value[6..].split('-').collect();
    if parts.is_empty() {
        return None;
    }
    let start_part = parts[0];
    let end_part = parts.get(1).copied().unwrap_or("");

    // 多段范围（bytes=0-100,200-300）不在支持范围内
    if end_part.contains(',') {
        return None;
    }

    if start_part.is_empty() {
        // 后缀范围 bytes=-N：返回最后 N 字节
        let suffix_len = end_part.parse::<u64>().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = file_size.saturating_sub(suffix_len);
        return Some((start, file_size - 1));
    }

    let start = start_part.parse::<u64>().ok()?;
    if start >= file_size {
        return None;
    }
    let end = if end_part.is_empty() {
        file_size - 1
    } else {
        end_part.parse::<u64>().ok()?.min(file_size - 1)
    };
    if start > end {
        return None;
    }
    Some((start, end))
}

/// 处理视频文件请求
///
/// 解析 URL 中的视频路径，验证安全性后返回文件内容。
/// 支持 Range 请求头，返回 206 Partial Content 响应；
/// 无法满足的 Range 请求返回 416。
///
/// 路径校验包含两道关卡：目录包含（防路径穿越）与扩展名白名单。
/// 缺少后者时本端点会退化成"共享目录的通用文件下载器"——目录里的
/// `.txt`/`.db`/配置等任意文件都能被局域网客户端取走，而桌面端的
/// `play_video` 是校验扩展名的。
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

    // 仅允许受支持的视频扩展名，避免把共享目录变成任意文件下载端点
    let extension = video_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    if !extension
        .as_deref()
        .is_some_and(is_supported_video_extension)
    {
        log::warn!(
            "[HTTP服务器] 拒绝非视频文件请求: {:?}",
            video_path.file_name()
        );
        return super::response::text_response(403, "Access denied: not a supported video file");
    }

    if !video_path.exists() {
        return super::response::text_response(404, "File not found");
    }

    let file = match File::open(&video_path) {
        Ok(f) => f,
        Err(_) => return super::response::text_response(404, "File not found"),
    };

    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

    let requested_bytes_range = range_header.is_some_and(|h| h.starts_with("bytes="));

    let (range_start, range_end, has_range) = match parse_range(range_header, file_size) {
        Some((start, end)) => (start, end, true),
        None if requested_bytes_range => {
            // 请求了 bytes 范围但无法满足 → 416
            return super::response::range_not_satisfiable_response(file_size);
        }
        None => (0, file_size.saturating_sub(1), false),
    };

    let content_length = if file_size == 0 {
        0
    } else {
        range_end.saturating_sub(range_start) + 1
    };

    let mut file = file;
    if content_length > 0 && file.seek(SeekFrom::Start(range_start)).is_err() {
        return super::response::text_response(500, "Seek error");
    }

    let content_type = video_content_type(
        video_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(""),
    );

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
        tiny_http::Header::from_bytes(
            &b"Cache-Control"[..],
            &b"private, max-age=3600, must-revalidate"[..],
        )
        .unwrap(),
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

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn test_parse_range_no_header() {
        assert_eq!(parse_range(None, 1000), None);
    }

    #[test]
    fn test_parse_range_interval() {
        assert_eq!(parse_range(Some("bytes=0-499"), 1000), Some((0, 499)));
        assert_eq!(parse_range(Some("bytes=500-"), 1000), Some((500, 999)));
        assert_eq!(parse_range(Some("bytes=999-999"), 1000), Some((999, 999)));
    }

    #[test]
    fn test_parse_range_suffix() {
        assert_eq!(parse_range(Some("bytes=-500"), 1000), Some((500, 999)));
        assert_eq!(parse_range(Some("bytes=-0"), 1000), None);
        assert_eq!(parse_range(Some("bytes=-2000"), 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_end_beyond_file() {
        // 结束超出文件大小时截断到末尾
        assert_eq!(parse_range(Some("bytes=0-2000"), 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_unsatisfiable() {
        // 起始超出文件大小
        assert_eq!(parse_range(Some("bytes=1000-"), 1000), None);
        assert_eq!(parse_range(Some("bytes=1001-2000"), 1000), None);
        // 起始大于结束
        assert_eq!(parse_range(Some("bytes=500-499"), 1000), None);
        // 非数字
        assert_eq!(parse_range(Some("bytes=abc-def"), 1000), None);
        // 多段范围不在支持范围内
        assert_eq!(parse_range(Some("bytes=0-100,200-300"), 1000), None);
        // 空文件
        assert_eq!(parse_range(Some("bytes=0-"), 0), None);
        // 非 bytes 单位
        assert_eq!(parse_range(Some("items=0-100"), 1000), None);
    }
}
