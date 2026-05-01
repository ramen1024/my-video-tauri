use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Take};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;

use walkdir::WalkDir;

mod error;
mod models;
mod password;
mod utils;

pub use error::AppError;
pub use models::{ShareServerInfo, VideoFile};
pub use password::PasswordStatus;
pub use utils::{format_system_time, get_local_ips, is_root_directory, sanitize_video_path};

static CANCEL_SCAN_FLAG: once_cell::sync::Lazy<Arc<AtomicBool>> =
    once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));

static SHARED_VIDEOS: once_cell::sync::Lazy<Arc<RwLock<Vec<VideoFile>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

static SHARED_FOLDER_PATH: once_cell::sync::Lazy<Arc<RwLock<String>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(String::new())));

static LOGIN_PAGE: &str = include_str!("login_template.html");

static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

static SERVER_HANDLE: once_cell::sync::Lazy<Arc<RwLock<Option<Arc<tiny_http::Server>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

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

    use std::collections::HashSet;
    use once_cell::sync::Lazy;

    static VIDEO_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
        [
            "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
        ]
        .iter()
        .cloned()
        .collect()
    });

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

            if !VIDEO_EXTENSIONS.contains(ext_lower.as_str()) {
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
    videos.sort_by(|a, b| {
        a.name
            .chars()
            .flat_map(|c| c.to_lowercase())
            .cmp(b.name.chars().flat_map(|c| c.to_lowercase()))
    });

    let mut shared = SHARED_VIDEOS.write();
    *shared = videos;

    let mut shared_path = SHARED_FOLDER_PATH.write();
    *shared_path = folder_path;

    Ok(())
}

#[tauri::command]
fn get_shared_videos() -> Result<Vec<VideoFile>, AppError> {
    Ok(SHARED_VIDEOS.read().clone())
}

#[tauri::command]
fn cancel_scan() {
    CANCEL_SCAN_FLAG.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn play_video(file_path: String) -> Result<(), AppError> {
    tauri_plugin_opener::open_path(&file_path, None::<&str>)
        .map_err(|e| AppError::IoError(format!("无法打开视频: {}", e)))
}

#[tauri::command]
fn get_password_status() -> PasswordStatus {
    password::get_password_status()
}

#[tauri::command]
fn set_password_enabled(enabled: bool) -> Result<(), AppError> {
    if enabled && !password::has_password_set() {
        return Err(AppError::PasswordError(
            "请先设置密码再启用密码保护".to_string(),
        ));
    }
    password::set_password_enabled(enabled);
    let status = if enabled { "开启" } else { "关闭" };
    println!("[安全审计] 密码保护功能已{} - 时间: {}", status, chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    Ok(())
}

#[tauri::command]
fn set_password(password: String) -> Result<(), AppError> {
    password::set_password(&password).map_err(AppError::PasswordError)
}

#[tauri::command]
fn verify_password_cmd(password: String) -> Result<bool, AppError> {
    password::verify_password(&password).map_err(AppError::PasswordError)
}

#[tauri::command]
fn generate_random_password() -> String {
    password::generate_random_password()
}

#[tauri::command]
fn reset_password() -> Result<(), AppError> {
    password::reset_password();
    println!("[安全审计] 密码已重置 - 时间: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    Ok(())
}

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

    SERVER_RUNNING.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        let addr = format!("0.0.0.0:{}", port);

        match tiny_http::Server::http(&addr) {
            Ok(server) => {
                let server = Arc::new(server);
                {
                    let mut handle = SERVER_HANDLE.write();
                    *handle = Some(server.clone());
                }
                println!("Share server started at http://{:?}:{}", ips_clone, port);

                let mut request_count: u64 = 0;
                for mut request in server.incoming_requests() {
                    if !SERVER_RUNNING.load(Ordering::SeqCst) {
                        break;
                    }

                    request_count += 1;
                    if request_count % 100 == 0 {
                        password::cleanup_expired_sessions();
                    }

                    let url = request.url().to_string();
                    let method = request.method().clone();

                    if url == "/auth" && method == tiny_http::Method::Post {
                        let response = handle_auth_request(&mut request);
                        request.respond(response).ok();
                        continue;
                    }

                    if password::is_password_enabled() {
                        let cookie_header = request
                            .headers()
                            .iter()
                            .find(|h| h.field.as_str() == "Cookie")
                            .map(|h| h.value.as_str())
                            .unwrap_or("");

                        if !password::check_web_auth(cookie_header) {
                            if url == "/login" || url == "/login.html" {
                                let html = LOGIN_PAGE.to_string();
                                let html_bytes = html.into_bytes();
                                let html_len = html_bytes.len();
                                let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(html_bytes));
                                let response = tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Content-Length"[..], html_len.to_string().as_bytes()).unwrap(),
                                    ],
                                    cursor,
                                    Some(html_len),
                                    None,
                                );
                                request.respond(response).ok();
                                continue;
                            }

                            let redirect_html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><script>window.location.href='/login';</script></head><body></body></html>"#;
                            let html_bytes = redirect_html.as_bytes().to_vec();
                            let html_len = html_bytes.len();
                            let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(html_bytes));
                            let response = tiny_http::Response::new(
                                302.into(),
                                vec![
                                    tiny_http::Header::from_bytes(&b"Location"[..], &b"/login"[..]).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Content-Length"[..], html_len.to_string().as_bytes()).unwrap(),
                                ],
                                cursor,
                                Some(html_len),
                                None,
                            );
                            request.respond(response).ok();
                            continue;
                        }
                    }

                    let response: tiny_http::Response<Box<dyn Read + Send>> = 'response: {
                        match url.as_str() {
                            "/" | "/index.html" => {
                                let videos = SHARED_VIDEOS.read().clone();
                                let html = generate_html(&videos, &ips_clone, port);
                                let html_bytes = html.into_bytes();
                                let html_len = html_bytes.len();
                                let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(html_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Content-Length"[..], html_len.to_string().as_bytes()).unwrap(),
                                    ],
                                    cursor,
                                    Some(html_len),
                                    None,
                                );
                            }
                            "/videos" => {
                                let videos = SHARED_VIDEOS.read().clone();
                                let json = serde_json::to_string(&videos).unwrap();
                                let json_bytes = json.into_bytes();
                                let json_len = json_bytes.len();
                                let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Content-Length"[..], json_len.to_string().as_bytes()).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap(),
                                    ],
                                    cursor,
                                    Some(json_len),
                                    None,
                                );
                            }
                            "/refresh" => {
                                let folder_path = SHARED_FOLDER_PATH.read().clone();
                                let result = if !folder_path.is_empty() {
                                    match scan_videos(folder_path) {
                                        Ok(_) => r#"{"success": true, "message": "视频列表已刷新"}"#.to_string(),
                                        Err(e) => format!(r#"{{"success": false, "message": "{}"}}"#, e),
                                    }
                                } else {
                                    r#"{"success": false, "message": "未设置共享文件夹"}"#.to_string()
                                };
                                let json_bytes = result.into_bytes();
                                let json_len = json_bytes.len();
                                let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
                                break 'response tiny_http::Response::new(
                                    200.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Content-Length"[..], json_len.to_string().as_bytes()).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                                    ],
                                    cursor,
                                    Some(json_len),
                                    None,
                                );
                            }
                            _ if url.starts_with("/video/") => {
                                let video_name = url.strip_prefix("/video/").unwrap_or("");
                                if video_name.is_empty() {
                                    break 'response make_text_response(400, "Invalid video path");
                                }

                                let folder_path = SHARED_FOLDER_PATH.read().clone();

                                let video_path = match sanitize_video_path(
                                    Path::new(&folder_path),
                                    video_name,
                                ) {
                                    Some(path) => path,
                                    None => {
                                        break 'response make_text_response(403, "Access denied: invalid path");
                                    }
                                };

                                println!("Video request: {} -> {}", video_name, video_path.display());

                                if video_path.exists() {
                                    let file = File::open(&video_path);
                                    if let Ok(mut file) = file {
                                        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

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
                                                let range_parts: Vec<&str> = range_value[6..].split('-').collect();
                                                if let Some(start) = range_parts.first() {
                                                    if !start.is_empty() {
                                                        range_start = start.parse().unwrap_or(0);
                                                    }
                                                }
                                                if let Some(end) = range_parts.get(1) {
                                                    if !end.is_empty() {
                                                        range_end = end.parse().unwrap_or(file_size.saturating_sub(1));
                                                    }
                                                }
                                            }
                                        }

                                        let content_length = range_end.saturating_sub(range_start) + 1;
                                        println!(
                                            "Serving bytes {}-{} / {} (length: {})",
                                            range_start, range_end, file_size, content_length
                                        );

                                        if file.seek(SeekFrom::Start(range_start)).is_ok() {
                                            let content_type = match video_path.extension().and_then(|e| e.to_str()) {
                                                Some("mp4") => "video/mp4",
                                                Some("webm") => "video/webm",
                                                Some("mkv") => "video/x-matroska",
                                                Some("avi") => "video/x-msvideo",
                                                Some("mov") => "video/quicktime",
                                                _ => "application/octet-stream",
                                            };

                                            let limited_reader: Take<File> = file.take(content_length);
                                            let boxed_reader: Box<dyn Read + Send> = Box::new(limited_reader);

                                            break 'response tiny_http::Response::new(
                                                if has_range { 206 } else { 200 }.into(),
                                                vec![
                                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                                                    tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                                                    tiny_http::Header::from_bytes(&b"Content-Range"[..], format!("bytes {}-{}/{}", range_start, range_end, file_size).as_bytes()).unwrap(),
                                                    tiny_http::Header::from_bytes(&b"Content-Length"[..], content_length.to_string().as_bytes()).unwrap(),
                                                    tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
                                                ],
                                                boxed_reader,
                                                Some(content_length as usize),
                                                None,
                                            );
                                        } else {
                                            break 'response make_text_response(500, "Seek error");
                                        }
                                    } else {
                                        break 'response make_text_response(404, "File not found");
                                    }
                                } else {
                                    break 'response make_text_response(404, "File not found");
                                }
                            }
                            _ => {
                                break 'response make_text_response(404, "Not found");
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

fn make_text_response(status_code: u16, message: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let err_bytes = message.as_bytes().to_vec();
    let err_len = err_bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(err_bytes));
    tiny_http::Response::new(
        status_code.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], err_len.to_string().as_bytes()).unwrap(),
        ],
        cursor,
        Some(err_len),
        None,
    )
}

fn handle_auth_request(request: &mut tiny_http::Request) -> tiny_http::Response<Box<dyn Read + Send>> {
    let mut body = String::new();
    if let Some(len) = request.body_length() {
        let mut limited = request.as_reader().take(len as u64);
        let _ = limited.read_to_string(&mut body);
    }

    let ip = request.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();

    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(data) => {
            let password = data["password"].as_str().unwrap_or("");
            if password.is_empty() {
                let json = r#"{"success": false, "message": "请输入密码"}"#;
                return make_json_response(400, json);
            }

            match password::authenticate_web_request(&ip, password) {
                Ok(token) => {
                    println!("[安全审计] 密码验证成功 - IP: {} - 时间: {}", ip, chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
                    let json = format!(r#"{{"success": true, "token": "{}"}}"#, token);
                    let json_bytes = json.into_bytes();
                    let json_len = json_bytes.len();
                    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
                    tiny_http::Response::new(
                        200.into(),
                        vec![
                            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                            tiny_http::Header::from_bytes(&b"Content-Length"[..], json_len.to_string().as_bytes()).unwrap(),
                            tiny_http::Header::from_bytes(
                                &b"Set-Cookie"[..],
                                format!("session_token={}; Path=/; Max-Age=3600; HttpOnly; SameSite=Strict", token).as_bytes(),
                            ).unwrap(),
                        ],
                        cursor,
                        Some(json_len),
                        None,
                    )
                }
                Err(e) => {
                    println!("[安全审计] 密码验证失败 - IP: {} - 时间: {}", ip, chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
                    let json = format!(r#"{{"success": false, "message": "{}"}}"#, e);
                    make_json_response(401, &json)
                }
            }
        }
        Err(_) => {
            let json = r#"{"success": false, "message": "无效的请求数据"}"#;
            make_json_response(400, json)
        }
    }
}

fn make_json_response(status_code: u16, json: &str) -> tiny_http::Response<Box<dyn Read + Send>> {
    let json_bytes = json.as_bytes().to_vec();
    let json_len = json_bytes.len();
    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
    tiny_http::Response::new(
        status_code.into(),
        vec![
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            tiny_http::Header::from_bytes(&b"Content-Length"[..], json_len.to_string().as_bytes()).unwrap(),
            tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        ],
        cursor,
        Some(json_len),
        None,
    )
}

#[tauri::command]
fn stop_share_server() -> Result<(), AppError> {
    SERVER_RUNNING.store(false, Ordering::SeqCst);
    {
        let mut handle = SERVER_HANDLE.write();
        if let Some(server) = handle.take() {
            server.unblock();
        }
    }
    Ok(())
}

#[tauri::command]
fn get_server_status() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}

fn generate_html(_videos: &[VideoFile], ips: &[String], port: u16) -> String {
    let addresses: String = ips
        .iter()
        .map(|ip| format!(r#"<span class="address-item">http://{}:{}</span>"#, ip, port))
        .collect::<Vec<_>>()
        .join(" | ");

    let template = include_str!("html_template.html");
    template.replace("{addresses}", &addresses)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    password::load_password_config();

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
            get_server_status,
            get_password_status,
            set_password_enabled,
            set_password,
            verify_password_cmd,
            generate_random_password,
            reset_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
