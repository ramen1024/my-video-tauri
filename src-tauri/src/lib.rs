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
    let video_data: String = videos
        .iter()
        .map(|v| {
            let video_url = format!("/video/{}", urlencoding_encode(&v.relative_path));
            format!(
                r#"{{"name":"{}","url":"{}","size":{},"modified":"{}","extension":"{}"}}"#,
                v.name.replace("\"", "\\\""),
                video_url,
                v.size,
                v.modified.as_ref().map(|s| s.as_str()).unwrap_or(""),
                v.extension
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let addresses: String = ips
        .iter()
        .map(|ip| format!(r#"<span class="address-item">http://{}:{}</span>"#, ip, port))
        .collect::<Vec<_>>()
        .join(" | ");

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
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }}
        .container {{
            max-width: 900px;
            margin: 0 auto;
        }}
        .header {{
            text-align: center;
            margin-bottom: 30px;
            color: #fff;
        }}
        .header h1 {{
            font-size: 2.5em;
            margin-bottom: 10px;
            text-shadow: 2px 2px 4px rgba(0,0,0,0.2);
        }}
        .header .subtitle {{
            opacity: 0.9;
            font-size: 1.1em;
        }}
        .addresses {{
            background: rgba(255,255,255,0.2);
            backdrop-filter: blur(10px);
            padding: 15px 25px;
            border-radius: 50px;
            display: inline-block;
            margin-top: 15px;
        }}
        .address-item {{
            color: #fff;
            font-weight: 500;
        }}
        .toolbar {{
            background: #fff;
            border-radius: 12px;
            padding: 15px 20px;
            margin-bottom: 20px;
            box-shadow: 0 4px 15px rgba(0,0,0,0.1);
            display: flex;
            justify-content: space-between;
            align-items: center;
            flex-wrap: wrap;
            gap: 10px;
        }}
        .search-box {{
            flex: 1;
            min-width: 200px;
            max-width: 300px;
        }}
        .search-box input {{
            width: 100%;
            padding: 10px 15px;
            border: 2px solid #e0e0e0;
            border-radius: 25px;
            font-size: 14px;
            transition: border-color 0.3s;
        }}
        .search-box input:focus {{
            outline: none;
            border-color: #667eea;
        }}
        .sort-buttons {{
            display: flex;
            gap: 8px;
            flex-wrap: wrap;
        }}
        .sort-btn {{
            padding: 8px 16px;
            border: 2px solid #e0e0e0;
            border-radius: 20px;
            background: #fff;
            color: #666;
            cursor: pointer;
            font-size: 13px;
            transition: all 0.3s;
        }}
        .sort-btn:hover {{
            border-color: #667eea;
            color: #667eea;
        }}
        .sort-btn.active {{
            background: #667eea;
            border-color: #667eea;
            color: #fff;
        }}
        .stats {{
            background: #fff;
            border-radius: 12px;
            padding: 15px 20px;
            margin-bottom: 20px;
            box-shadow: 0 4px 15px rgba(0,0,0,0.1);
            display: flex;
            justify-content: space-around;
            flex-wrap: wrap;
            gap: 15px;
        }}
        .stat-item {{
            text-align: center;
        }}
        .stat-value {{
            font-size: 1.8em;
            font-weight: 700;
            color: #667eea;
        }}
        .stat-label {{
            font-size: 0.85em;
            color: #888;
            margin-top: 5px;
        }}
        .video-list {{
            background: #fff;
            border-radius: 12px;
            overflow: hidden;
            box-shadow: 0 4px 15px rgba(0,0,0,0.1);
        }}
        .video-item {{
            display: grid;
            grid-template-columns: auto 1fr auto auto;
            align-items: center;
            padding: 15px 20px;
            border-bottom: 1px solid #f0f0f0;
            transition: background 0.2s;
            cursor: pointer;
        }}
        .video-item:hover {{
            background: #f8f9ff;
            transform: translateX(4px);
        }}
        .video-item:last-child {{
            border-bottom: none;
        }}
        .video-icon {{
            width: 40px;
            height: 40px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            border-radius: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 15px;
            font-size: 18px;
        }}
        .video-info {{
            flex: 1;
            min-width: 0;
        }}
        .video-name {{
            font-weight: 600;
            color: #333;
            margin-bottom: 4px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }}
        .video-meta {{
            font-size: 12px;
            color: #888;
        }}
        .video-meta span {{
            margin-right: 15px;
        }}
        .video-ext {{
            background: #f0f0f0;
            padding: 4px 10px;
            border-radius: 4px;
            font-size: 11px;
            font-weight: 600;
            color: #666;
            text-transform: uppercase;
            margin-left: 15px;
        }}
        .video-size {{
            font-weight: 600;
            color: #667eea;
            font-size: 14px;
            margin-left: 15px;
            text-align: right;
            min-width: 70px;
        }}
        .empty {{
            text-align: center;
            padding: 60px 20px;
            color: #888;
        }}
        .empty-icon {{
            font-size: 48px;
            margin-bottom: 15px;
        }}
        .player-overlay {{
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.9);
            backdrop-filter: blur(10px);
            z-index: 1000;
            justify-content: center;
            align-items: center;
        }}
        .player-container {{
            width: 90%;
            max-width: 1000px;
            background: #1a1a1a;
            border-radius: 16px;
            overflow: hidden;
            box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
        }}
        .player-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 12px 16px;
            background: #2d2d2d;
        }}
        .player-title {{
            color: #fff;
            font-size: 14px;
            font-weight: 500;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            flex: 1;
            margin-right: 16px;
        }}
        .close-btn {{
            background: transparent;
            border: none;
            color: #fff;
            cursor: pointer;
            padding: 8px 12px;
            border-radius: 6px;
            font-size: 18px;
            transition: background 0.2s;
        }}
        .close-btn:hover {{
            background: rgba(255, 255, 255, 0.1);
        }}
        .video-player {{
            width: 100%;
            display: block;
            max-height: 75vh;
            background: #000;
        }}
        @media (max-width: 600px) {{
            .video-item {{
                grid-template-columns: auto 1fr auto;
            }}
            .video-ext {{
                display: none;
            }}
            .toolbar {{
                flex-direction: column;
            }}
            .search-box {{
                max-width: 100%;
            }}
            .player-container {{
                width: 100%;
                border-radius: 0;
            }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📹 视频扫描器</h1>
            <div class="subtitle">局域网视频共享服务</div>
            <div class="addresses">{}</div>
        </div>
        
        <div class="toolbar">
            <div class="search-box">
                <input type="text" id="searchInput" placeholder="🔍 搜索视频..." oninput="filterVideos()">
            </div>
            <div class="sort-buttons">
                <button class="sort-btn active" onclick="sortVideos('name', 'asc')">名称 ↑</button>
                <button class="sort-btn" onclick="sortVideos('name', 'desc')">名称 ↓</button>
                <button class="sort-btn" onclick="sortVideos('size', 'desc')">大小 ↓</button>
                <button class="sort-btn" onclick="sortVideos('size', 'asc')">大小 ↑</button>
                <button class="sort-btn" onclick="sortVideos('modified', 'desc')">时间 ↓</button>
                <button class="sort-btn" onclick="sortVideos('modified', 'asc')">时间 ↑</button>
            </div>
        </div>
        
        <div class="stats">
            <div class="stat-item">
                <div class="stat-value" id="totalCount">0</div>
                <div class="stat-label">视频数量</div>
            </div>
            <div class="stat-item">
                <div class="stat-value" id="totalSize">0 B</div>
                <div class="stat-label">总大小</div>
            </div>
        </div>
        
        <div class="video-list" id="videoList"></div>
        
        <div class="player-overlay" id="playerOverlay" onclick="closePlayer(event)">
            <div class="player-container" onclick="event.stopPropagation()">
                <div class="player-header">
                    <span class="player-title" id="playerTitle"></span>
                    <button class="close-btn" onclick="closePlayer()">&#10005;</button>
                </div>
                <video id="videoPlayer" controls autoplay class="video-player">
                    您的浏览器不支持视频播放
                </video>
            </div>
        </div>
    </div>
    
    <script>
        const videos = [{}];
        let currentSort = {{ field: 'name', order: 'asc' }};
        let searchTerm = '';
        
        function formatSize(bytes) {{
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
        }}
        
        function getExtIcon(ext) {{
            const icons = {{
                'mp4': '🎬', 'mkv': '🎬', 'avi': '🎬', 'mov': '🎬',
                'wmv': '🎬', 'flv': '🎬', 'webm': '🎬', 'm4v': '🎬',
                'mpg': '🎬', 'mpeg': '🎬'
            }};
            return icons[ext] || '📹';
        }}
        
        function openPlayer(url, name) {{
            document.getElementById('playerTitle').textContent = name;
            document.getElementById('videoPlayer').src = url;
            document.getElementById('playerOverlay').style.display = 'flex';
            document.body.style.overflow = 'hidden';
        }}
        
        function closePlayer(event) {{
            if (event && event.target !== event.currentTarget) return;
            document.getElementById('playerOverlay').style.display = 'none';
            document.getElementById('videoPlayer').pause();
            document.getElementById('videoPlayer').src = '';
            document.body.style.overflow = '';
        }}
        
        function renderVideos() {{
            let filtered = videos.filter(v => 
                v.name.toLowerCase().includes(searchTerm.toLowerCase())
            );
            
            filtered.sort((a, b) => {{
                let valA, valB;
                if (currentSort.field === 'name') {{
                    valA = a.name.toLowerCase();
                    valB = b.name.toLowerCase();
                }} else if (currentSort.field === 'size') {{
                    valA = a.size;
                    valB = b.size;
                }} else {{
                    valA = a.modified || '';
                    valB = b.modified || '';
                }}
                
                if (valA < valB) return currentSort.order === 'asc' ? -1 : 1;
                if (valA > valB) return currentSort.order === 'asc' ? 1 : -1;
                return 0;
            }});
            
            const list = document.getElementById('videoList');
            
            if (filtered.length === 0) {{
                list.innerHTML = '<div class="empty"><div class="empty-icon">🔍</div><div>没有找到视频</div></div>';
                return;
            }}
            
            list.innerHTML = filtered.map(v => `
                <div class="video-item" onclick="openPlayer('${{v.url}}', '${{v.name.replace(/'/g, "\\\\'")}}')">
                    <div class="video-icon">${{getExtIcon(v.extension)}}</div>
                    <div class="video-info">
                        <div class="video-name">${{v.name}}</div>
                        <div class="video-meta">
                            <span>📅 ${{v.modified || '未知'}}</span>
                        </div>
                    </div>
                    <div class="video-ext">${{v.extension}}</div>
                    <div class="video-size">${{formatSize(v.size)}}</div>
                </div>
            `).join('');
            
            document.getElementById('totalCount').textContent = filtered.length;
            const totalBytes = filtered.reduce((sum, v) => sum + v.size, 0);
            document.getElementById('totalSize').textContent = formatSize(totalBytes);
        }}
        
        function sortVideos(field, order) {{
            currentSort = {{ field, order }};
            document.querySelectorAll('.sort-btn').forEach(btn => btn.classList.remove('active'));
            event.target.classList.add('active');
            renderVideos();
        }}
        
        function filterVideos() {{
            searchTerm = document.getElementById('searchInput').value;
            renderVideos();
        }}
        
        document.addEventListener('keydown', function(e) {{
            if (e.key === 'Escape') closePlayer();
        }});
        
        renderVideos();
    </script>
</body>
</html>"#, addresses, video_data)
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
