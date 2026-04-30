use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Take};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use walkdir::WalkDir;

mod error;
mod models;
mod utils;

pub use error::AppError;
pub use models::{ShareServerInfo, VideoFile};
pub use utils::{format_system_time, get_local_ips, is_root_directory, sanitize_video_path};

// ============================================
// 全局状态
// ============================================

/// 扫描取消标志 - 用于中途取消扫描操作
static CANCEL_SCAN_FLAG: once_cell::sync::Lazy<Arc<AtomicBool>> =
    once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));

/// 共享视频列表 - 用于 HTTP 服务器动态刷新
static SHARED_VIDEOS: once_cell::sync::Lazy<Arc<RwLock<Vec<VideoFile>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// 共享文件夹路径 - 用于重新扫描
static SHARED_FOLDER_PATH: once_cell::sync::Lazy<Arc<RwLock<String>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(String::new())));

/// 服务器运行状态标志
static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

// ============================================
// Tauri 命令函数
// ============================================

/// 扫描文件夹中的视频文件
#[tauri::command]
fn scan_videos(folder_path: String) -> Result<(), AppError> {
    use rayon::prelude::*;

    let path = Path::new(&folder_path);

    if !path.exists() {
        return Err(AppError::InvalidPath("文件夹不存在".to_string()));
    }
    if !path.is_dir() {
        return Err(AppError::InvalidPath("路径不是文件夹".to_string()));
    }

    if is_root_directory(path) {
        return Err(AppError::InvalidPath(
            "扫描磁盘根目录可能会花费大量时间并导致程序卡住，请选择一个具体的文件夹".to_string(),
        ));
    }

    let video_extensions: std::collections::HashSet<&str> = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
    ]
    .iter()
    .cloned()
    .collect();

    let base_path = Path::new(&folder_path).to_path_buf();

    CANCEL_SCAN_FLAG.store(false, Ordering::SeqCst);

    let entries: Vec<_> = WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();

    if CANCEL_SCAN_FLAG.load(Ordering::SeqCst) {
        return Err(AppError::ScanCancelled);
    }

    let videos: Vec<VideoFile> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if CANCEL_SCAN_FLAG.load(Ordering::SeqCst) {
                return None;
            }

            let path = entry.path();

            let ext_lower = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())?;

            if !video_extensions.contains(ext_lower.as_str()) {
                return None;
            }

            let metadata = fs::metadata(path).ok()?;
            let size = metadata.len();

            if size < 1_048_576 {
                return None;
            }

            let modified = metadata.modified().ok().map(format_system_time);

            let relative_path = path
                .strip_prefix(&base_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            Some(VideoFile {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                relative_path,
                size,
                modified,
                extension: ext_lower,
            })
        })
        .collect();

    if CANCEL_SCAN_FLAG.load(Ordering::SeqCst) {
        return Err(AppError::ScanCancelled);
    }

    let mut videos = videos;
    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    if let Ok(mut shared) = SHARED_VIDEOS.write() {
        *shared = videos;
    }

    if let Ok(mut shared_path) = SHARED_FOLDER_PATH.write() {
        *shared_path = folder_path;
    }

    Ok(())
}

/// 获取共享视频列表
#[tauri::command]
fn get_shared_videos() -> Result<Vec<VideoFile>, AppError> {
    SHARED_VIDEOS
        .read()
        .map(|videos| videos.clone())
        .map_err(|_| AppError::IoError("无法读取视频列表".to_string()))
}

/// 取消正在进行的扫描操作
#[tauri::command]
fn cancel_scan() {
    CANCEL_SCAN_FLAG.store(true, Ordering::SeqCst);
}

/// 使用系统默认播放器播放视频
///
/// # 安全改进
/// 使用 Tauri 的 opener 插件替代手动执行系统命令，避免命令注入风险。
#[tauri::command]
fn play_video(file_path: String) -> Result<(), AppError> {
    tauri_plugin_opener::open_path(&file_path, None::<&str>)
        .map_err(|e| AppError::IoError(format!("无法打开视频: {}", e)))
}

/// 启动共享服务器
#[tauri::command]
fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, AppError> {
    if SERVER_RUNNING.load(Ordering::SeqCst) {
        return Err(AppError::ServerAlreadyRunning);
    }

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath("无效的文件夹路径".to_string()));
    }

    scan_videos(folder_path.clone())?;
    let videos = get_shared_videos()?;
    let ips = get_local_ips();

    let ips_clone = ips.clone();
    let folder_path_clone = folder_path.clone();

    SERVER_RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        let addr = format!("0.0.0.0:{}", port);

        match tiny_http::Server::http(&addr) {
            Ok(server) => {
                println!("Share server started at http://{:?}:{}", ips_clone, port);

                for request in server.incoming_requests() {
                    if !SERVER_RUNNING.load(Ordering::SeqCst) {
                        break;
                    }

                    let url = request.url();

                    let response: tiny_http::Response<Box<dyn Read + Send>> = 'response: {
                        match url {
                            "/" | "/index.html" => {
                                let videos = SHARED_VIDEOS
                                    .read()
                                    .map(|v| v.clone())
                                    .unwrap_or_default();
                                let html = generate_html(&videos, &ips_clone, port);
                                let html_bytes = html.into_bytes();
                                let html_len = html_bytes.len();
                                let cursor: Box<dyn Read + Send> =
                                    Box::new(Cursor::new(html_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Type"[..],
                                            &b"text/html; charset=utf-8"[..],
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Length"[..],
                                            html_len.to_string().as_bytes(),
                                        )
                                        .unwrap(),
                                    ],
                                    cursor,
                                    Some(html_len),
                                    None,
                                );
                            }
                            "/videos" => {
                                let videos = SHARED_VIDEOS
                                    .read()
                                    .map(|v| v.clone())
                                    .unwrap_or_default();
                                let json = serde_json::to_string(&videos).unwrap();
                                let json_bytes = json.into_bytes();
                                let json_len = json_bytes.len();
                                let cursor: Box<dyn Read + Send> =
                                    Box::new(Cursor::new(json_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Type"[..],
                                            &b"application/json"[..],
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Length"[..],
                                            json_len.to_string().as_bytes(),
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Access-Control-Allow-Origin"[..],
                                            &b"*"[..],
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Cache-Control"[..],
                                            &b"no-cache, no-store, must-revalidate"[..],
                                        )
                                        .unwrap(),
                                    ],
                                    cursor,
                                    Some(json_len),
                                    None,
                                );
                            }
                            "/refresh" => {
                                let folder_path = SHARED_FOLDER_PATH
                                    .read()
                                    .map(|p| p.clone())
                                    .unwrap_or_default();
                                let result = if !folder_path.is_empty() {
                                    match scan_videos(folder_path) {
                                        Ok(_) => {
                                            r#"{"success": true, "message": "视频列表已刷新"}"#
                                        }
                                        Err(e) => &format!(
                                            r#"{{"success": false, "message": "{}"}}"#,
                                            e
                                        ),
                                    }
                                } else {
                                    r#"{"success": false, "message": "未设置共享文件夹"}"#
                                };
                                let json_bytes = result.as_bytes().to_vec();
                                let json_len = json_bytes.len();
                                let cursor: Box<dyn Read + Send> =
                                    Box::new(Cursor::new(json_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Type"[..],
                                            &b"application/json"[..],
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Length"[..],
                                            json_len.to_string().as_bytes(),
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Access-Control-Allow-Origin"[..],
                                            &b"*"[..],
                                        )
                                        .unwrap(),
                                    ],
                                    cursor,
                                    Some(json_len),
                                    None,
                                );
                            }
                            _ if url.starts_with("/video/") => {
                                let video_name = url.strip_prefix("/video/").unwrap_or("");
                                if video_name.is_empty() {
                                    let err_msg = "Invalid video path";
                                    let err_bytes = err_msg.as_bytes().to_vec();
                                    let err_len = err_bytes.len();
                                    let cursor: Box<dyn Read + Send> =
                                        Box::new(Cursor::new(err_bytes));
                                    break 'response tiny_http::Response::new(
                                        400.into(),
                                        vec![
                                            tiny_http::Header::from_bytes(
                                                &b"Content-Type"[..],
                                                &b"text/plain"[..],
                                            )
                                            .unwrap(),
                                            tiny_http::Header::from_bytes(
                                                &b"Content-Length"[..],
                                                err_len.to_string().as_bytes(),
                                            )
                                            .unwrap(),
                                        ],
                                        cursor,
                                        Some(err_len),
                                        None,
                                    );
                                }

                                let folder_path = SHARED_FOLDER_PATH
                                    .read()
                                    .map(|p| p.clone())
                                    .unwrap_or_else(|_| folder_path_clone.clone());

                                let video_path = match sanitize_video_path(
                                    Path::new(&folder_path),
                                    video_name,
                                ) {
                                    Some(path) => path,
                                    None => {
                                        let err_msg = "Access denied: invalid path";
                                        let err_bytes = err_msg.as_bytes().to_vec();
                                        let err_len = err_bytes.len();
                                        let cursor: Box<dyn Read + Send> =
                                            Box::new(Cursor::new(err_bytes));
                                        break 'response tiny_http::Response::new(
                                            403.into(),
                                            vec![
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Type"[..],
                                                    &b"text/plain"[..],
                                                )
                                                .unwrap(),
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Length"[..],
                                                    err_len.to_string().as_bytes(),
                                                )
                                                .unwrap(),
                                            ],
                                            cursor,
                                            Some(err_len),
                                            None,
                                        );
                                    }
                                };

                                println!(
                                    "Video request: {} -> {}",
                                    video_name,
                                    video_path.display()
                                );

                                if video_path.exists() {
                                    let file = File::open(&video_path);
                                    if let Ok(mut file) = file {
                                        let file_size =
                                            file.metadata().map(|m| m.len()).unwrap_or(0);

                                        let mut range_start = 0u64;
                                        let mut range_end = file_size.saturating_sub(1);
                                        let mut has_range = false;

                                        if let Some(range_header) = request
                                            .headers()
                                            .iter()
                                            .find(|h| h.field.as_str() == "Range")
                                        {
                                            let range_value = range_header.value.as_str();
                                            println!("Range request: {}", range_value);
                                            has_range = true;
                                            if range_value.starts_with("bytes=") {
                                                let range_parts: Vec<&str> =
                                                    range_value[6..].split('-').collect();
                                                if let Some(start) = range_parts.first() {
                                                    if !start.is_empty() {
                                                        range_start =
                                                            start.parse().unwrap_or(0);
                                                    }
                                                }
                                                if let Some(end) = range_parts.get(1) {
                                                    if !end.is_empty() {
                                                        range_end = end
                                                            .parse()
                                                            .unwrap_or(file_size.saturating_sub(1));
                                                    }
                                                }
                                            }
                                        }

                                        let content_length =
                                            range_end.saturating_sub(range_start) + 1;
                                        println!(
                                            "Serving bytes {}-{} / {} (length: {})",
                                            range_start, range_end, file_size, content_length
                                        );

                                        if file.seek(SeekFrom::Start(range_start)).is_ok() {
                                            let content_type = match video_path
                                                .extension()
                                                .and_then(|e| e.to_str())
                                            {
                                                Some("mp4") => "video/mp4",
                                                Some("webm") => "video/webm",
                                                Some("mkv") => "video/x-matroska",
                                                Some("avi") => "video/x-msvideo",
                                                Some("mov") => "video/quicktime",
                                                _ => "application/octet-stream",
                                            };

                                            let limited_reader: Take<File> =
                                                file.take(content_length);
                                            let boxed_reader: Box<dyn Read + Send> =
                                                Box::new(limited_reader);

                                            break 'response tiny_http::Response::new(
                                                if has_range { 206 } else { 200 }.into(),
                                                vec![
                                                    tiny_http::Header::from_bytes(
                                                        &b"Content-Type"[..],
                                                        content_type.as_bytes(),
                                                    )
                                                    .unwrap(),
                                                    tiny_http::Header::from_bytes(
                                                        &b"Accept-Ranges"[..],
                                                        &b"bytes"[..],
                                                    )
                                                    .unwrap(),
                                                    tiny_http::Header::from_bytes(
                                                        &b"Content-Range"[..],
                                                        format!(
                                                            "bytes {}-{}/{}",
                                                            range_start, range_end, file_size
                                                        )
                                                        .as_bytes(),
                                                    )
                                                    .unwrap(),
                                                    tiny_http::Header::from_bytes(
                                                        &b"Content-Length"[..],
                                                        content_length.to_string().as_bytes(),
                                                    )
                                                    .unwrap(),
                                                    tiny_http::Header::from_bytes(
                                                        &b"Cache-Control"[..],
                                                        &b"no-cache"[..],
                                                    )
                                                    .unwrap(),
                                                ],
                                                boxed_reader,
                                                Some(content_length as usize),
                                                None,
                                            );
                                        } else {
                                            let err_msg = "Seek error".to_string();
                                            let err_bytes = err_msg.into_bytes();
                                            let err_len = err_bytes.len();
                                            let cursor: Box<dyn Read + Send> =
                                                Box::new(Cursor::new(err_bytes));
                                            break 'response tiny_http::Response::new(
                                                500.into(),
                                                vec![
                                                    tiny_http::Header::from_bytes(
                                                        &b"Content-Type"[..],
                                                        &b"text/plain"[..],
                                                    )
                                                    .unwrap(),
                                                    tiny_http::Header::from_bytes(
                                                        &b"Content-Length"[..],
                                                        err_len.to_string().as_bytes(),
                                                    )
                                                    .unwrap(),
                                                ],
                                                cursor,
                                                Some(err_len),
                                                None,
                                            );
                                        }
                                    } else {
                                        let err_msg = "File not found".to_string();
                                        let err_bytes = err_msg.into_bytes();
                                        let err_len = err_bytes.len();
                                        let cursor: Box<dyn Read + Send> =
                                            Box::new(Cursor::new(err_bytes));
                                        break 'response tiny_http::Response::new(
                                            404.into(),
                                            vec![
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Type"[..],
                                                    &b"text/plain"[..],
                                                )
                                                .unwrap(),
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Length"[..],
                                                    err_len.to_string().as_bytes(),
                                                )
                                                .unwrap(),
                                            ],
                                            cursor,
                                            Some(err_len),
                                            None,
                                        );
                                    }
                                } else {
                                    let err_msg = "File not found".to_string();
                                    let err_bytes = err_msg.into_bytes();
                                    let err_len = err_bytes.len();
                                    let cursor: Box<dyn Read + Send> =
                                        Box::new(Cursor::new(err_bytes));
                                    break 'response tiny_http::Response::new(
                                        404.into(),
                                        vec![
                                            tiny_http::Header::from_bytes(
                                                &b"Content-Type"[..],
                                                &b"text/plain"[..],
                                            )
                                            .unwrap(),
                                            tiny_http::Header::from_bytes(
                                                &b"Content-Length"[..],
                                                err_len.to_string().as_bytes(),
                                            )
                                            .unwrap(),
                                        ],
                                        cursor,
                                        Some(err_len),
                                        None,
                                    );
                                }
                            }
                            _ => {
                                let err_msg = "Not found".to_string();
                                let err_bytes = err_msg.into_bytes();
                                let err_len = err_bytes.len();
                                let cursor: Box<dyn Read + Send> =
                                    Box::new(Cursor::new(err_bytes));
                                break 'response tiny_http::Response::new(
                                    404.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Type"[..],
                                            &b"text/plain"[..],
                                        )
                                        .unwrap(),
                                        tiny_http::Header::from_bytes(
                                            &b"Content-Length"[..],
                                            err_len.to_string().as_bytes(),
                                        )
                                        .unwrap(),
                                    ],
                                    cursor,
                                    Some(err_len),
                                    None,
                                );
                            }
                        }
                    };

                    request.respond(response).ok();
                }
            }
            Err(e) => {
                println!("Failed to start server: {}", e);
            }
        }
    });

    Ok(ShareServerInfo { ips, port, videos })
}

/// 停止共享服务器
#[tauri::command]
fn stop_share_server() -> Result<(), AppError> {
    SERVER_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}

/// 获取服务器运行状态
#[tauri::command]
fn get_server_status() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}

/// 生成网页端 HTML
fn generate_html(_videos: &[VideoFile], ips: &[String], port: u16) -> String {
    let addresses: String = ips
        .iter()
        .map(|ip| format!(r#"<span class="address-item">http://{}:{}</span>"#, ip, port))
        .collect::<Vec<_>>()
        .join(" | ");

    let template = include_str!("html_template.html");
    template.replace("{addresses}", &addresses)
}

// ============================================
// 应用入口
// ============================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            scan_videos,
            get_shared_videos,
            play_video,
            cancel_scan,
            start_share_server,
            stop_share_server,
            get_server_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
