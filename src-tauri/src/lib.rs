use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::fs::Metadata;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoFile {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub size: u64,
    pub modified: Option<String>,
    pub extension: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShareServerInfo {
    pub ips: Vec<String>,
    pub port: u16,
    pub videos: Vec<VideoFile>,
}

static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

fn format_system_time(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn is_root_directory(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        if path_str.len() == 3 && path_str.chars().nth(1) == Some(':') {
            return true;
        }
        if path_str == "\\" || path_str == "/" {
            return true;
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if path.to_string_lossy() == "/" {
            return true;
        }
    }
    false
}

fn get_local_ips() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("cmd")
            .args(["/C", "ipconfig"])
            .output();
        
        let mut ips = Vec::new();
        if let Ok(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("IPv4") && line.contains(":") {
                    if let Some(ip) = line.split(':').nth(1) {
                        let ip = ip.trim();
                        if !ip.is_empty() && !ip.starts_with("127") {
                            ips.push(ip.to_string());
                        }
                    }
                }
            }
        }
        if ips.is_empty() {
            ips.push("127.0.0.1".to_string());
        }
        ips
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec!["127.0.0.1".to_string()]
    }
}

#[tauri::command]
fn scan_videos(folder_path: String) -> Result<Vec<VideoFile>, String> {
    let path = Path::new(&folder_path);
    if !path.exists() {
        return Err("文件夹不存在".to_string());
    }
    if !path.is_dir() {
        return Err("路径不是文件夹".to_string());
    }

    if is_root_directory(path) {
        return Err("警告：扫描磁盘根目录可能会花费大量时间并导致程序卡住，请选择一个具体的文件夹".to_string());
    }

    let video_extensions = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
    ];

    let mut videos: Vec<VideoFile> = Vec::new();
    let base_path = Path::new(&folder_path);

    for entry in WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if video_extensions.contains(&ext_lower.as_str()) {
                    let metadata: Option<Metadata> = fs::metadata(path).ok();
                    let size = metadata.as_ref().map(|m: &Metadata| m.len()).unwrap_or(0);
                    let modified = metadata
                        .and_then(|m: Metadata| m.modified().ok())
                        .map(format_system_time);

                    let relative_path = path.strip_prefix(base_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    videos.push(VideoFile {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.to_string_lossy().to_string(),
                        relative_path,
                        size,
                        modified,
                        extension: ext_lower,
                    });
                }
            }
        }
    }

    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(videos)
}

#[tauri::command]
fn play_video(file_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &file_path])
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, String> {
    if SERVER_RUNNING.load(Ordering::SeqCst) {
        return Err("服务器已在运行".to_string());
    }

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err("无效的文件夹路径".to_string());
    }

    let video_extensions = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
    ];

    let mut videos: Vec<VideoFile> = Vec::new();
    let base_path = Path::new(&folder_path);

    for entry in WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if video_extensions.contains(&ext_lower.as_str()) {
                    let metadata: Option<Metadata> = fs::metadata(path).ok();
                    let size = metadata.as_ref().map(|m: &Metadata| m.len()).unwrap_or(0);
                    let modified = metadata
                        .and_then(|m: Metadata| m.modified().ok())
                        .map(format_system_time);

                    let relative_path = path.strip_prefix(base_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    videos.push(VideoFile {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.to_string_lossy().to_string(),
                        relative_path,
                        size,
                        modified,
                        extension: ext_lower,
                    });
                }
            }
        }
    }

    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let ips = get_local_ips();
    let ips_clone = ips.clone();
    let videos_clone = videos.clone();
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
                    let response = match url {
                        "/" | "/index.html" => {
                            let html = generate_html(&videos_clone, &ips_clone, port);
                            tiny_http::Response::from_string(html)
                                .with_header(
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                                )
                        }
                        "/videos" => {
                            let json = serde_json::to_string(&videos_clone).unwrap();
                            tiny_http::Response::from_string(json)
                                .with_header(
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
                                )
                                .with_header(
                                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
                                )
                        }
                        _ if url.starts_with("/video/") => {
                            let video_name = url.strip_prefix("/video/").unwrap();
                            let video_name_decoded = urlencoding_decode(video_name);
                            
                            let video_name_fixed = video_name_decoded.replace("/", "\\");
                            let video_path = Path::new(&folder_path_clone).join(&video_name_fixed);
                            
                            println!("Video request: {} -> {} -> {}", video_name, video_name_decoded, video_path.display());
                            
                            if video_path.exists() {
                                let file = File::open(&video_path);
                                if let Ok(mut file) = file {
                                    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                                    
                                    let mut range_start = 0u64;
                                    let mut range_end = file_size - 1;
                                    let mut has_range = false;
                                    
                                    if let Some(range_header) = request.headers().iter().find(|h| h.field.as_str() == "Range") {
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
                                                    range_end = end.parse().unwrap_or(file_size - 1);
                                                }
                                            }
                                        }
                                    }

                                    let content_length = range_end - range_start + 1;
                                    println!("Serving bytes {}-{} / {} (length: {})", range_start, range_end, file_size, content_length);
                                    
                                    file.seek(SeekFrom::Start(range_start)).ok();
                                    
                                    let mut buffer = vec![0u8; content_length as usize];
                                    if file.read(&mut buffer).is_ok() {
                                        let content_type = match video_path.extension().and_then(|e| e.to_str()) {
                                            Some("mp4") => "video/mp4",
                                            Some("webm") => "video/webm",
                                            Some("mkv") => "video/x-matroska",
                                            Some("avi") => "video/x-msvideo",
                                            Some("mov") => "video/quicktime",
                                            _ => "application/octet-stream",
                                        };

                                        tiny_http::Response::from_data(buffer)
                                            .with_header(
                                                tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap()
                                            )
                                            .with_header(
                                                tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap()
                                            )
                                            .with_header(
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Range"[..],
                                                    format!("bytes {}-{}/{}", range_start, range_end, file_size).as_bytes()
                                                ).unwrap()
                                            )
                                            .with_header(
                                                tiny_http::Header::from_bytes(&b"Content-Length"[..], content_length.to_string().as_bytes()).unwrap()
                                            )
                                            .with_status_code(if has_range {
                                                206
                                            } else {
                                                200
                                            })
                                    } else {
                                        tiny_http::Response::from_string("Error reading file")
                                            .with_status_code(500)
                                    }
                                } else {
                                    tiny_http::Response::from_string("File not found")
                                        .with_status_code(404)
                                }
                            } else {
                                tiny_http::Response::from_string("File not found")
                                    .with_status_code(404)
                            }
                        }
                        _ => {
                            tiny_http::Response::from_string("Not found")
                                .with_status_code(404)
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

    Ok(ShareServerInfo {
        ips,
        port,
        videos,
    })
}

fn urlencoding_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                bytes.push(byte);
            }
        } else if c == '+' {
            bytes.push(b' ');
        } else {
            bytes.extend(c.to_string().as_bytes());
        }
    }
    
    String::from_utf8_lossy(&bytes).to_string()
}

#[tauri::command]
fn stop_share_server() -> Result<(), String> {
    SERVER_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn get_server_status() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}

fn generate_html(videos: &[VideoFile], ips: &[String], port: u16) -> String {
    let video_items: String = videos
        .iter()
        .map(|v| {
            let video_url = format!("/video/{}", urlencoding_encode(&v.relative_path));
            format!(
                r#"<div class="video-item">
                    <a href="{}" target="_blank">▶ {}</a>
                    <span class="size">{}</span>
                </div>"#,
                video_url,
                v.name,
                format_size(v.size)
            )
        })
        .collect();

    let addresses: String = ips
        .iter()
        .map(|ip| format!(r#"<div class="address-item">http://{}:{}</div>"#, ip, port))
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>视频扫描器</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: #f5f5f5;
            color: #333;
            padding: 20px;
        }}
        @media (prefers-color-scheme: dark) {{
            body {{ background: #1a1a1a; color: #fff; }}
            .video-item {{ background: #2d2d2d; }}
            .video-item a {{ color: #60a5fa; }}
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
        }}
        .header h1 {{
            color: #0078d4;
            margin-bottom: 10px;
        }}
        .addresses {{
            background: #e1f5fe;
            padding: 15px 20px;
            border-radius: 8px;
            display: inline-block;
            margin-bottom: 20px;
        }}
        .address-item {{
            word-break: break-all;
            padding: 5px 0;
        }}
        @media (prefers-color-scheme: dark) {{
            .addresses {{ background: #1a237e; }}
        }}
        .video-list {{
            max-width: 800px;
            margin: 0 auto;
        }}
        .video-item {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 15px 20px;
            background: #fff;
            border-radius: 8px;
            margin-bottom: 10px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .video-item a {{
            color: #0078d4;
            text-decoration: none;
            font-size: 16px;
            flex: 1;
        }}
        .video-item a:hover {{
            text-decoration: underline;
        }}
        .size {{
            color: #666;
            font-size: 14px;
            margin-left: 20px;
        }}
        @media (prefers-color-scheme: dark) {{
            .size {{ color: #999; }}
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>📹 视频扫描器</h1>
        <div class="addresses">{}</div>
    </div>
    <div class="video-list">
        {}
    </div>
</body>
</html>"#, addresses, video_items)
}

fn urlencoding_encode(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as i32;
    format!("{:.2} {}", bytes as f64 / k.powi(i), sizes[i as usize])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            scan_videos, 
            play_video,
            start_share_server,
            stop_share_server,
            get_server_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
