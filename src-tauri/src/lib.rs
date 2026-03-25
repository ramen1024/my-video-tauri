// ============================================
// 导入依赖库
// ============================================

// serde: Rust 的序列化/反序列化框架，用于 JSON 等数据格式转换
// Serialize: 将结构体转换为 JSON 等格式
// Deserialize: 将 JSON 等格式转换为结构体
use serde::{Deserialize, Serialize};

// std::fs: 文件系统操作模块
use std::fs;
use std::fs::File;
use std::fs::Metadata;

// std::io: 输入输出操作
// Read: 读取数据的 trait
// Seek: 文件指针定位的 trait
// SeekFrom: 定位方式的枚举（Start, End, Current）
// Take: 限制读取字节数的包装器
// Cursor: 内存中的可读写缓冲区
use std::io::{Cursor, Read, Seek, SeekFrom, Take};

// std::path::Path: 路径处理，跨平台兼容
use std::path::Path;

// std::process::Command: 执行外部命令/程序
use std::process::Command;

// std::sync::atomic: 原子操作，用于线程安全的共享状态
// AtomicBool: 原子布尔值，多线程安全
// Ordering: 内存排序规则，控制原子操作的可见性
use std::sync::atomic::{AtomicBool, Ordering};

// std::time::SystemTime: 系统时间类型
use std::time::SystemTime;

// walkdir: 第三方库，递归遍历目录
use walkdir::WalkDir;

// lazy_static: 用于创建全局静态变量
use std::sync::Arc;
use std::sync::RwLock;

// ============================================
// 全局状态
// ============================================

/// 扫描取消标志 - 用于中途取消扫描操作
/// Arc<AtomicBool>: 线程安全的共享布尔值
static CANCEL_SCAN_FLAG: once_cell::sync::Lazy<Arc<AtomicBool>> = 
    once_cell::sync::Lazy::new(|| Arc::new(AtomicBool::new(false)));

/// 共享视频列表 - 用于 HTTP 服务器动态刷新
/// Arc<RwLock<>>: 线程安全的读写锁，允许多个读取者或一个写入者
static SHARED_VIDEOS: once_cell::sync::Lazy<Arc<RwLock<Vec<VideoFile>>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// 共享文件夹路径 - 用于重新扫描
static SHARED_FOLDER_PATH: once_cell::sync::Lazy<Arc<RwLock<String>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(String::new())));

// ============================================
// 数据结构定义
// ============================================

/// 视频文件信息结构体
/// 
/// # 属性说明
/// - `#[derive(Debug)]`: 自动实现 Debug trait，支持 {:?} 格式化输出
/// - `#[derive(Serialize)]`: 自动实现序列化，可转换为 JSON
/// - `#[derive(Deserialize)]`: 自动实现反序列化，可从 JSON 解析
/// - `#[derive(Clone)]`: 允许克隆（深拷贝）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoFile {
    /// 文件名（不含路径）
    pub name: String,
    /// 完整绝对路径
    pub path: String,
    /// 相对于扫描目录的相对路径（用于网页端访问）
    pub relative_path: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（可选，可能获取失败）
    pub modified: Option<String>,
    /// 文件扩展名（小写）
    pub extension: String,
}

/// 共享服务器信息结构体
/// 返回给前端，包含服务器地址和视频列表
#[derive(Debug, Serialize, Deserialize)]
pub struct ShareServerInfo {
    /// 本机所有可用的 IP 地址列表
    pub ips: Vec<String>,
    /// 服务器监听端口
    pub port: u16,
    /// 共享的视频列表
    pub videos: Vec<VideoFile>,
}

// ============================================
// 全局状态
// ============================================

/// 服务器运行状态标志
/// 
/// static: 静态变量，程序运行期间一直存在
/// AtomicBool: 原子布尔值，线程安全
/// 
/// 为什么用原子类型？
/// - HTTP 服务器在独立线程中运行
/// - 主线程需要能够停止服务器
/// - 原子类型确保多线程访问安全
static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

// ============================================
// 辅助函数
// ============================================

/// 将系统时间格式化为可读字符串
/// 
/// # 参数
/// - `time`: SystemTime 类型的时间
/// 
/// # 返回
/// 格式化后的字符串，如 "2024-01-15 14:30:00"
fn format_system_time(time: SystemTime) -> String {
    // chrono: 第三方日期时间库
    // DateTime<Local>: 本地时区的日期时间
    let datetime: chrono::DateTime<chrono::Local> = time.into();
    // format! 宏: 格式化字符串
    // %Y: 四位年份, %m: 两位月份, %d: 两位日期
    // %H: 24小时制小时, %M: 分钟, %S: 秒
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 检查路径是否为磁盘根目录
/// 
/// # 为什么需要这个检查？
/// 扫描根目录会遍历整个磁盘，非常耗时且可能导致程序卡住
/// 
/// # 参数
/// - `path`: 要检查的路径
/// 
/// # 返回
/// true 表示是根目录
fn is_root_directory(path: &Path) -> bool {
    // #[cfg(target_os = "windows")]: 条件编译，仅在 Windows 平台编译此代码块
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        // Windows 根目录格式: "C:\" (3个字符，第2个是冒号)
        if path_str.len() == 3 && path_str.chars().nth(1) == Some(':') {
            return true;
        }
        // UNC 路径根目录
        if path_str == "\\" || path_str == "/" {
            return true;
        }
    }
    // #[cfg(not(target_os = "windows"))]: 非 Windows 平台
    #[cfg(not(target_os = "windows"))]
    {
        // Unix/Linux 根目录是 "/"
        if path.to_string_lossy() == "/" {
            return true;
        }
    }
    false
}

/// 获取本机所有可用的 IP 地址
/// 
/// # 实现方式
/// Windows: 执行 ipconfig 命令并解析输出
/// 
/// # 返回
/// IP 地址列表，过滤掉 127.x.x.x 回环地址
fn get_local_ips() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        // Command: 执行外部命令
        // .new("cmd"): 创建命令实例
        // .args(): 传递参数
        // .output(): 执行并捕获输出
        let output = Command::new("cmd")
            .args(["/C", "ipconfig"])
            .output();
        
        let mut ips = Vec::new();
        // Result 处理: Ok 成功, Err 失败
        if let Ok(output) = output {
            // String::from_utf8_lossy: 将字节转换为字符串，遇到无效 UTF-8 时替换为占位符
            let output_str = String::from_utf8_lossy(&output.stdout);
            // 逐行解析
            for line in output_str.lines() {
                // 查找 IPv4 地址行，格式: "   IPv4 地址 . . . : 192.168.1.1"
                if line.contains("IPv4") && line.contains(":") {
                    // split(':'): 按冒号分割
                    // nth(1): 取第二个元素（索引从0开始）
                    if let Some(ip) = line.split(':').nth(1) {
                        let ip = ip.trim();
                        // 过滤空值和回环地址
                        if !ip.is_empty() && !ip.starts_with("127") {
                            ips.push(ip.to_string());
                        }
                    }
                }
            }
        }
        // 如果没找到任何 IP，返回本地回环地址
        if ips.is_empty() {
            ips.push("127.0.0.1".to_string());
        }
        ips
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows 平台暂返回默认值
        vec!["127.0.0.1".to_string()]
    }
}

// ============================================
// Tauri 命令函数
// ============================================

/// 扫描文件夹中的视频文件
/// 
/// # Tauri 命令说明
/// `#[tauri::command]`: 将函数标记为 Tauri 命令
/// - 可从前端 JavaScript 调用: invoke('scan_videos', { folderPath: '...' })
/// - 自动处理参数类型转换
/// - 返回值自动序列化为 JSON
/// 
/// # 参数
/// - `folder_path`: 要扫描的文件夹路径
/// 
/// # 返回
/// - `Ok(())`: 扫描成功
/// - `Err(String)`: 错误信息
#[tauri::command]
fn scan_videos(folder_path: String) -> Result<(), String> {
    // Path::new(): 从字符串创建路径对象
    let path = Path::new(&folder_path);
    
    // 路径验证
    if !path.exists() {
        return Err("文件夹不存在".to_string());
    }
    if !path.is_dir() {
        return Err("路径不是文件夹".to_string());
    }

    // 根目录检查
    if is_root_directory(path) {
        return Err("警告：扫描磁盘根目录可能会花费大量时间并导致程序卡住，请选择一个具体的文件夹".to_string());
    }

    // 支持的视频扩展名数组
    let video_extensions = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
    ];

    // Vec: 动态数组（可变长度列表）
    let mut videos: Vec<VideoFile> = Vec::new();
    let base_path = Path::new(&folder_path);

    // 重置取消标志
    CANCEL_SCAN_FLAG.store(false, Ordering::SeqCst);
    
    // WalkDir: 递归遍历目录
    // .follow_links(true): 跟随符号链接
    // .into_iter(): 转换为迭代器
    // .filter_map(|e| e.ok()): 过滤掉错误，只保留成功的条目
    for entry in WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        // 检查是否被取消
        if CANCEL_SCAN_FLAG.load(Ordering::SeqCst) {
            return Err("扫描已取消".to_string());
        }
        
        let path = entry.path();
        
        // 只处理文件，跳过目录
        if path.is_file() {
            // path.extension(): 获取文件扩展名
            // Option 类型: Some(ext) 或 None
            // if let Some(ext) = ...: 模式匹配，仅当有值时执行
            if let Some(ext) = path.extension() {
                // to_string_lossy(): 处理非 UTF-8 路径
                let ext_lower = ext.to_string_lossy().to_lowercase();
                
                // 检查是否为视频文件
                if video_extensions.contains(&ext_lower.as_str()) {
                    // 获取文件元数据
                    // Option<Metadata>: 可能获取失败
                    let metadata: Option<Metadata> = fs::metadata(path).ok();
                    
                    // map(): 转换 Option 内部的值
                    let size = metadata.as_ref().map(|m: &Metadata| m.len()).unwrap_or(0);
                    
                    // 过滤小于1MB的文件 (1MB = 1024 * 1024 = 1048576 bytes)
                    if size < 1_048_576 {
                        continue;
                    }
                    
                    // and_then(): 链式 Option 操作
                    let modified = metadata
                        .and_then(|m: Metadata| m.modified().ok())
                        .map(format_system_time);

                    // 计算相对路径
                    // strip_prefix(): 移除路径前缀
                    let relative_path = path.strip_prefix(base_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // 创建 VideoFile 实例
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

    // 按文件名排序（忽略大小写）
    // sort_by(): 自定义排序
    // cmp(): 比较两个值
    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // 更新共享视频列表（单数据源）
    if let Ok(mut shared) = SHARED_VIDEOS.write() {
        *shared = videos;
    }
    
    // 更新共享文件夹路径
    if let Ok(mut shared_path) = SHARED_FOLDER_PATH.write() {
        *shared_path = folder_path;
    }

    Ok(())
}

/// 获取共享视频列表
/// 
/// # 功能
/// 从单数据源 SHARED_VIDEOS 读取视频列表
/// 
/// # 返回
/// - `Ok(Vec<VideoFile>)`: 视频文件列表
#[tauri::command]
fn get_shared_videos() -> Result<Vec<VideoFile>, String> {
    SHARED_VIDEOS.read()
        .map(|videos| videos.clone())
        .map_err(|_| "无法读取视频列表".to_string())
}

/// 取消正在进行的扫描操作
/// 
/// # 功能
/// 设置取消标志，使扫描循环提前退出
#[tauri::command]
fn cancel_scan() {
    CANCEL_SCAN_FLAG.store(true, Ordering::SeqCst);
}

/// 使用系统默认播放器播放视频
/// 
/// # 跨平台实现
/// - Windows: 使用 `cmd /C start` 命令
/// - macOS: 使用 `open` 命令
/// - Linux: 使用 `xdg-open` 命令
#[tauri::command]
fn play_video(file_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // spawn(): 启动子进程，不等待完成
        // map_err(): 将错误转换为自定义错误信息
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



/// 启动共享服务器
///
/// # HTTP 服务器路由
/// - `/`: 主页（视频列表）
/// - `/videos`: 视频列表 JSON API
/// - `/refresh`: 刷新视频列表 API
/// - `/video/<path>`: 视频文件流
#[tauri::command]
fn start_share_server(folder_path: String, port: u16) -> Result<ShareServerInfo, String> {
    // 检查服务器是否已在运行
    if SERVER_RUNNING.load(Ordering::SeqCst) {
        return Err("服务器已在运行".to_string());
    }

    let path = Path::new(&folder_path);
    if !path.exists() || !path.is_dir() {
        return Err("无效的文件夹路径".to_string());
    }

    // 扫描视频文件（scan_videos 会自动更新 SHARED_VIDEOS 和 SHARED_FOLDER_PATH）
    scan_videos(folder_path.clone())?;
    
    // 从单数据源获取视频列表
    let videos = get_shared_videos()?;

    // 获取本机 IP 地址
    let ips = get_local_ips();
    
    // 克隆数据用于新线程
    let ips_clone = ips.clone();
    let folder_path_clone = folder_path.clone();

    // 设置服务器运行状态
    SERVER_RUNNING.store(true, Ordering::SeqCst);

    // std::thread::spawn: 创建新线程
    // move 闭包: 获取捕获变量的所有权
    std::thread::spawn(move || {
        // 监听地址: 0.0.0.0 表示所有网络接口
        let addr = format!("0.0.0.0:{}", port);
        
        // tiny_http: 轻量级 HTTP 服务器库
        match tiny_http::Server::http(&addr) {
            Ok(server) => {
                println!("Share server started at http://{:?}:{}", ips_clone, port);
                
                // incoming_requests(): 迭代接收请求
                for request in server.incoming_requests() {
                    // 检查是否应该停止
                    if !SERVER_RUNNING.load(Ordering::SeqCst) {
                        break;
                    }

                    let url = request.url();
                    
                    // 路由匹配 - 所有响应统一为 Box<dyn Read + Send> 类型
                    let response: tiny_http::Response<Box<dyn Read + Send>> = match url {
                        // 主页
                        "/" | "/index.html" => {
                            // 从共享状态读取视频列表
                            let videos = SHARED_VIDEOS.read().map(|v| v.clone()).unwrap_or_default();
                            let html = generate_html(&videos, &ips_clone, port);
                            let html_bytes = html.into_bytes();
                            let html_len = html_bytes.len();
                            let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(html_bytes));
                            tiny_http::Response::new(
                                200.into(),
                                vec![
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Content-Length"[..], html_len.to_string().as_bytes()).unwrap(),
                                ],
                                cursor,
                                Some(html_len),
                                None,
                            )
                        }
                        // 视频 API - 从共享状态读取
                        "/videos" => {
                            let videos = SHARED_VIDEOS.read().map(|v| v.clone()).unwrap_or_default();
                            let json = serde_json::to_string(&videos).unwrap();
                            let json_bytes = json.into_bytes();
                            let json_len = json_bytes.len();
                            let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
                            tiny_http::Response::new(
                                200.into(),
                                vec![
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Content-Length"[..], json_len.to_string().as_bytes()).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                                    // 禁用缓存，确保获取最新数据
                                    tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap(),
                                ],
                                cursor,
                                Some(json_len),
                                None,
                            )
                        }
                        // 刷新 API - 重新扫描文件夹
                        "/refresh" => {
                            // 先读取文件夹路径，然后释放读锁，避免死锁
                            let folder_path = SHARED_FOLDER_PATH.read().map(|p| p.clone()).unwrap_or_default();
                            // 注意：必须在这里释放读锁后才能调用 scan_videos，
                            // 因为 scan_videos 内部会获取写锁
                            let result = if !folder_path.is_empty() {
                                match scan_videos(folder_path) {
                                    Ok(_) => r#"{"success": true, "message": "视频列表已刷新"}"#,
                                    Err(e) => &format!(r#"{{"success": false, "message": "{}"}}"#, e),
                                }
                            } else {
                                r#"{"success": false, "message": "未设置共享文件夹"}"#
                            };
                            let json_bytes = result.as_bytes().to_vec();
                            let json_len = json_bytes.len();
                            let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(json_bytes));
                            tiny_http::Response::new(
                                200.into(),
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
                        // 视频文件流
                        _ if url.starts_with("/video/") => {
                            let video_name = url.strip_prefix("/video/").unwrap();
                            let video_name_decoded = urlencoding_decode(video_name);
                            
                            // Windows 路径分隔符转换
                            let video_name_fixed = video_name_decoded.replace("/", "\\");
                            
                            // 从共享状态获取最新的文件夹路径
                            let folder_path = SHARED_FOLDER_PATH.read()
                                .map(|p| p.clone())
                                .unwrap_or_else(|_| folder_path_clone.clone());
                            let video_path = Path::new(&folder_path).join(&video_name_fixed);
                            
                            println!("Video request: {} -> {} -> {}", video_name, video_name_decoded, video_path.display());
                            
                            if video_path.exists() {
                                let file = File::open(&video_path);
                                if let Ok(mut file) = file {
                                    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                                    
                                    // Range 请求处理（支持视频进度条拖动）
                                    let mut range_start = 0u64;
                                    let mut range_end = file_size.saturating_sub(1);
                                    let mut has_range = false;
                                    
                                    // 检查 Range 请求头
                                    // Range: bytes=0-1023 表示请求前 1024 字节
                                    if let Some(range_header) = request.headers().iter().find(|h| h.field.as_str() == "Range") {
                                        let range_value = range_header.value.as_str();
                                        println!("Range request: {}", range_value);
                                        has_range = true;
                                        if range_value.starts_with("bytes=") {
                                            // 解析范围: "bytes=0-1023" -> ["0", "1023"]
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
                                    println!("Serving bytes {}-{} / {} (length: {})", range_start, range_end, file_size, content_length);
                                    
                                    // 定位文件指针
                                    if file.seek(SeekFrom::Start(range_start)).is_ok() {
                                        // 根据扩展名确定 Content-Type
                                        let content_type = match video_path.extension().and_then(|e| e.to_str()) {
                                            Some("mp4") => "video/mp4",
                                            Some("webm") => "video/webm",
                                            Some("mkv") => "video/x-matroska",
                                            Some("avi") => "video/x-msvideo",
                                            Some("mov") => "video/quicktime",
                                            _ => "application/octet-stream",
                                        };

                                        // 使用 Take 限制读取长度，实现流式传输
                                        // 避免一次性读取大文件到内存
                                        let limited_reader: Take<File> = file.take(content_length);
                                        
                                        // 构建流式响应
                                        // Box::new 将具体类型转换为 trait 对象，实现动态分发
                                        let boxed_reader: Box<dyn Read + Send> = Box::new(limited_reader);
                                        
                                        tiny_http::Response::new(
                                            // 状态码: 206 Partial Content 或 200 OK
                                            if has_range { 206 } else { 200 }.into(),
                                            vec![
                                                tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                                                tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                                                tiny_http::Header::from_bytes(
                                                    &b"Content-Range"[..],
                                                    format!("bytes {}-{}/{}", range_start, range_end, file_size).as_bytes()
                                                ).unwrap(),
                                                tiny_http::Header::from_bytes(&b"Content-Length"[..], content_length.to_string().as_bytes()).unwrap(),
                                                // 禁用缓存，确保视频播放流畅
                                                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
                                            ],
                                            boxed_reader,
                                            Some(content_length as usize),
                                            None,
                                        )
                                    } else {
                                        // Seek 失败
                                        let err_msg = "Seek error".to_string();
                                        let err_bytes = err_msg.into_bytes();
                                        let err_len = err_bytes.len();
                                        let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(err_bytes));
                                        tiny_http::Response::new(
                                            500.into(),
                                            vec![
                                                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                                                tiny_http::Header::from_bytes(&b"Content-Length"[..], err_len.to_string().as_bytes()).unwrap(),
                                            ],
                                            cursor,
                                            Some(err_len),
                                            None,
                                        )
                                    }
                                } else {
                                    let err_msg = "File not found".to_string();
                                    let err_bytes = err_msg.into_bytes();
                                    let err_len = err_bytes.len();
                                    let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(err_bytes));
                                    tiny_http::Response::new(
                                        404.into(),
                                        vec![
                                            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                                            tiny_http::Header::from_bytes(&b"Content-Length"[..], err_len.to_string().as_bytes()).unwrap(),
                                        ],
                                        cursor,
                                        Some(err_len),
                                        None,
                                    )
                                }
                            } else {
                                let err_msg = "File not found".to_string();
                                let err_bytes = err_msg.into_bytes();
                                let err_len = err_bytes.len();
                                let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(err_bytes));
                                tiny_http::Response::new(
                                    404.into(),
                                    vec![
                                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                                        tiny_http::Header::from_bytes(&b"Content-Length"[..], err_len.to_string().as_bytes()).unwrap(),
                                    ],
                                    cursor,
                                    Some(err_len),
                                    None,
                                )
                            }
                        }
                        // 404 Not Found
                        _ => {
                            let err_msg = "Not found".to_string();
                            let err_bytes = err_msg.into_bytes();
                            let err_len = err_bytes.len();
                            let cursor: Box<dyn Read + Send> = Box::new(Cursor::new(err_bytes));
                            tiny_http::Response::new(
                                404.into(),
                                vec![
                                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                                    tiny_http::Header::from_bytes(&b"Content-Length"[..], err_len.to_string().as_bytes()).unwrap(),
                                ],
                                cursor,
                                Some(err_len),
                                None,
                            )
                        }
                    };

                    // 发送响应
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

/// URL 解码函数
/// 
/// # 为什么需要自定义实现？
/// 标准库的 URL 解码可能不支持多字节 UTF-8 字符（如中文）
/// 
/// # 实现原理
/// 1. 遇到 %XX 时，将 XX 解析为十六进制字节
/// 2. 收集所有字节后，转换为 UTF-8 字符串
fn urlencoding_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            // 取接下来的两个字符作为十六进制数
            let hex: String = chars.by_ref().take(2).collect();
            // from_str_radix: 从字符串解析指定进制的数字
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                bytes.push(byte);
            }
        } else if c == '+' {
            // URL 编码中 + 表示空格
            bytes.push(b' ');
        } else {
            // 普通字符直接添加
            bytes.extend(c.to_string().as_bytes());
        }
    }
    
    // from_utf8_lossy: 将字节转换为字符串，无效 UTF-8 替换为占位符
    String::from_utf8_lossy(&bytes).to_string()
}

/// 停止共享服务器
#[tauri::command]
fn stop_share_server() -> Result<(), String> {
    SERVER_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}

/// 获取服务器运行状态
#[tauri::command]
fn get_server_status() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}

/// 生成网页端 HTML
/// 
/// # 参数
/// - `videos`: 视频列表
/// - `ips`: IP 地址列表
/// - `port`: 端口号
/// 
/// # 返回
/// 完整的 HTML 页面字符串
fn generate_html(_videos: &[VideoFile], ips: &[String], port: u16) -> String {
    // 构建地址显示
    let addresses: String = ips
        .iter()
        .map(|ip| format!(r#"<span class="address-item">http://{}:{}</span>"#, ip, port))
        .collect::<Vec<_>>()
        .join(" | ");

    // format! 宏: 格式化字符串
    // {{ 和 }}: 转义大括号，输出字面量 { 和 }
    // {}: 占位符，按顺序替换
    format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>视频扫描器</title>
    <style>
        /* ============================================
           Glassmorphism (玻璃拟态) 主题样式
           设计特点:
           - 柔和渐变网格背景 (深蓝到紫色)
           - 毛玻璃效果 (backdrop-filter: blur)
           - 细薄优雅的分隔线
           - 圆角设计 (12px)
           - 柔和阴影 (营造悬浮层次感)
           - 未来感、高级、流畅
           ============================================ */
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        
        body {{
            font-family: "Segoe UI", "Microsoft YaHei", "PingFang SC", -apple-system, BlinkMacSystemFont, sans-serif;
            background: #0f172a;
            min-height: 100vh;
            padding: 24px;
            color: #ffffff;
            position: relative;
        }}
        
        .container {{
            max-width: 900px;
            margin: 0 auto;
            position: relative;
            z-index: 1;
        }}
        
        /* 头部 - Glassmorphism 风格 */
        .header {{
            text-align: center;
            margin-bottom: 24px;
            padding: 32px;
            background: rgba(255, 255, 255, 0.08);
            backdrop-filter: blur(20px);
            -webkit-backdrop-filter: blur(20px);
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 16px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
        }}
        
        .header h1 {{
            font-size: 2.2em;
            margin-bottom: 8px;
            font-weight: 600;
            color: #ffffff;
            letter-spacing: 0.5px;
            text-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
        }}
        
        .header .subtitle {{
            font-size: 1em;
            color: rgba(255, 255, 255, 0.7);
            letter-spacing: 0.5px;
        }}
        
        .addresses {{
            background: rgba(255, 255, 255, 0.1);
            backdrop-filter: blur(10px);
            -webkit-backdrop-filter: blur(10px);
            padding: 12px 24px;
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 12px;
            display: inline-block;
            margin-top: 16px;
            font-size: 13px;
        }}
        
        .address-item {{
            color: #60a5fa;
            font-weight: 500;
        }}
        
        /* 工具栏 - Glassmorphism 风格 */
        .toolbar {{
            background: rgba(255, 255, 255, 0.06);
            backdrop-filter: blur(20px);
            -webkit-backdrop-filter: blur(20px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 16px;
            padding: 20px 24px;
            margin-bottom: 20px;
            display: flex;
            justify-content: space-between;
            align-items: center;
            flex-wrap: wrap;
            gap: 16px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
        }}
        
        .search-box {{
            flex: 1;
            min-width: 200px;
            max-width: 300px;
            position: relative;
        }}
        
        .search-box input {{
            width: 100%;
            padding: 12px 16px 12px 40px;
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 10px;
            font-size: 14px;
            background: rgba(255, 255, 255, 0.08);
            color: #ffffff;
            transition: all 0.3s ease;
        }}
        
        .search-box input:focus {{
            outline: none;
            border-color: rgba(96, 165, 250, 0.5);
            background: rgba(255, 255, 255, 0.12);
            box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.1);
        }}
        
        .search-box input::placeholder {{
            color: rgba(255, 255, 255, 0.4);
        }}
        
        /* 排序按钮 - Glassmorphism 风格 */
        .sort-buttons {{
            display: flex;
            gap: 10px;
            flex-wrap: wrap;
        }}
        
        .sort-btn {{
            padding: 10px 18px;
            border: 1px solid rgba(255, 255, 255, 0.15);
            border-radius: 10px;
            background: rgba(255, 255, 255, 0.08);
            color: rgba(255, 255, 255, 0.7);
            cursor: pointer;
            font-size: 13px;
            font-weight: 500;
            transition: all 0.3s ease;
        }}
        
        .sort-btn:hover {{
            background: rgba(255, 255, 255, 0.15);
            border-color: rgba(96, 165, 250, 0.4);
            color: #60a5fa;
        }}
        
        .sort-btn.active {{
            background: rgba(96, 165, 250, 0.25);
            border-color: rgba(96, 165, 250, 0.5);
            color: #60a5fa;
        }}
        
        /* 统计面板 - Glassmorphism 风格 */
        .stats {{
            background: rgba(255, 255, 255, 0.06);
            backdrop-filter: blur(20px);
            -webkit-backdrop-filter: blur(20px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 16px;
            padding: 24px;
            margin-bottom: 20px;
            display: flex;
            justify-content: space-around;
            flex-wrap: wrap;
            gap: 20px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
        }}
        
        .stat-item {{
            text-align: center;
        }}
        
        .stat-value {{
            font-size: 2em;
            font-weight: 600;
            color: #60a5fa;
        }}
        
        .stat-label {{
            font-size: 0.85em;
            color: rgba(255, 255, 255, 0.6);
            margin-top: 4px;
            font-weight: 500;
        }}
        
        /* 视频列表 - Glassmorphism 风格 */
        .video-list {{
            background: rgba(255, 255, 255, 0.04);
            backdrop-filter: blur(10px);
            -webkit-backdrop-filter: blur(10px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            border-radius: 16px;
            overflow: hidden;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
        }}
        
        .video-item {{
            display: grid;
            grid-template-columns: auto 1fr auto auto;
            align-items: center;
            padding: 16px 20px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05);
            transition: all 0.2s ease;
            cursor: pointer;
        }}
        
        .video-item:hover {{
            background: rgba(255, 255, 255, 0.06);
        }}
        
        .video-item:last-child {{
            border-bottom: none;
        }}
        
        .video-icon {{
            width: 44px;
            height: 44px;
            background: rgba(96, 165, 250, 0.2);
            border: 1px solid rgba(96, 165, 250, 0.4);
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            margin-right: 16px;
            font-size: 18px;
            color: #60a5fa;
            transition: all 0.3s ease;
        }}
        
        .video-item:hover .video-icon {{
            background: rgba(96, 165, 250, 0.35);
            transform: scale(1.1);
        }}
        
        .video-info {{
            flex: 1;
            min-width: 0;
        }}
        
        .video-name {{
            font-weight: 500;
            color: rgba(255, 255, 255, 0.95);
            margin-bottom: 4px;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }}
        
        .video-meta {{
            font-size: 12px;
            color: rgba(255, 255, 255, 0.5);
        }}
        
        .video-meta span {{
            margin-right: 15px;
        }}
        
        .video-ext {{
            background: rgba(96, 165, 250, 0.15);
            padding: 5px 12px;
            border-radius: 6px;
            font-size: 11px;
            font-weight: 500;
            color: #60a5fa;
            text-transform: uppercase;
            margin-left: 15px;
        }}
        
        .video-size {{
            font-weight: 600;
            color: rgba(255, 255, 255, 0.8);
            font-size: 14px;
            margin-left: 15px;
            text-align: right;
            min-width: 70px;
        }}
        
        .empty {{
            text-align: center;
            padding: 60px 20px;
            color: rgba(255, 255, 255, 0.5);
        }}
        
        .empty-icon {{
            font-size: 48px;
            margin-bottom: 16px;
            opacity: 0.5;
        }}
        
        /* 播放器 - Glassmorphism 风格 */
        .player-overlay {{
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.6);
            backdrop-filter: blur(20px);
            -webkit-backdrop-filter: blur(20px);
            z-index: 1000;
            justify-content: center;
            align-items: center;
        }}
        
        .player-container {{
            width: 90%;
            max-width: 1000px;
            background: rgba(255, 255, 255, 0.1);
            backdrop-filter: blur(20px);
            -webkit-backdrop-filter: blur(20px);
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 16px;
            overflow: hidden;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
        }}
        
        .player-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 16px 24px;
            background: rgba(255, 255, 255, 0.05);
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }}
        
        .player-title {{
            color: #ffffff;
            font-size: 14px;
            font-weight: 500;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            flex: 1;
            margin-right: 16px;
        }}
        
        .close-btn {{
            background: rgba(255, 255, 255, 0.1);
            border: 1px solid rgba(255, 255, 255, 0.2);
            border-radius: 8px;
            color: #ffffff;
            cursor: pointer;
            padding: 10px 16px;
            font-size: 16px;
            font-weight: 500;
            transition: all 0.3s ease;
        }}
        
        .close-btn:hover {{
            background: rgba(239, 68, 68, 0.3);
            border-color: rgba(239, 68, 68, 0.5);
        }}
        
        .video-player {{
            width: 100%;
            display: block;
            max-height: 75vh;
            background: #000;
        }}
        
        /* 滚动条样式 */
        ::-webkit-scrollbar {{
            width: 8px;
            height: 8px;
        }}
        
        ::-webkit-scrollbar-track {{
            background: rgba(255, 255, 255, 0.03);
            border-radius: 4px;
        }}
        
        ::-webkit-scrollbar-thumb {{
            background: rgba(255, 255, 255, 0.15);
            border-radius: 4px;
        }}
        
        ::-webkit-scrollbar-thumb:hover {{
            background: rgba(255, 255, 255, 0.25);
        }}
        
        /* 响应式设计 */
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
                clip-path: none;
            }}
            .header h1 {{
                font-size: 1.8em;
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
                <button class="sort-btn" onclick="refreshVideos()" id="refreshBtn" title="刷新列表">🔄 刷新</button>
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
        // 视频数据数组 - 初始为空，从服务器获取
        let videos = [];
        let currentSort = {{ field: 'name', order: 'asc' }};
        let searchTerm = '';
        let isRefreshing = false;
        
        // 从服务器获取视频列表
        async function fetchVideos() {{
            try {{
                const response = await fetch('/videos', {{
                    cache: 'no-cache',
                    headers: {{ 'Cache-Control': 'no-cache' }}
                }});
                if (response.ok) {{
                    const data = await response.json();
                    videos = data;
                    renderVideos();
                }} else {{
                    console.error('获取视频列表失败:', response.status);
                }}
            }} catch (error) {{
                console.error('获取视频列表出错:', error);
            }}
        }}
        
        // 刷新视频列表（重新扫描文件夹）
        async function refreshVideos() {{
            if (isRefreshing) return;
            isRefreshing = true;
            
            const refreshBtn = document.getElementById('refreshBtn');
            refreshBtn.textContent = '⏳ 刷新中...';
            refreshBtn.disabled = true;
            
            try {{
                // 调用刷新 API
                const response = await fetch('/refresh', {{
                    method: 'POST',
                    cache: 'no-cache'
                }});
                
                if (response.ok) {{
                    const result = await response.json();
                    console.log('刷新结果:', result.message);
                    // 重新获取视频列表
                    await fetchVideos();
                }} else {{
                    console.error('刷新失败:', response.status);
                    alert('刷新失败，请重试');
                }}
            }} catch (error) {{
                console.error('刷新出错:', error);
                alert('刷新出错: ' + error.message);
            }} finally {{
                isRefreshing = false;
                refreshBtn.textContent = '🔄 刷新';
                refreshBtn.disabled = false;
            }}
        }}
        
        // 格式化文件大小
        function formatSize(bytes) {{
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
        }}
        
        // 获取扩展名图标
        function getExtIcon(ext) {{
            const icons = {{
                'mp4': '🎬', 'mkv': '🎬', 'avi': '🎬', 'mov': '🎬',
                'wmv': '🎬', 'flv': '🎬', 'webm': '🎬', 'm4v': '🎬',
                'mpg': '🎬', 'mpeg': '🎬'
            }};
            return icons[ext] || '📹';
        }}
        
        // 打开播放器弹窗
        function openPlayer(url, name) {{
            document.getElementById('playerTitle').textContent = name;
            document.getElementById('videoPlayer').src = url;
            document.getElementById('playerOverlay').style.display = 'flex';
            document.body.style.overflow = 'hidden';
        }}
        
        // 关闭播放器弹窗
        function closePlayer(event) {{
            if (event && event.target !== event.currentTarget) return;
            document.getElementById('playerOverlay').style.display = 'none';
            document.getElementById('videoPlayer').pause();
            document.getElementById('videoPlayer').src = '';
            document.body.style.overflow = '';
        }}
        
        // 渲染视频列表
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
                <div class="video-item" onclick="openPlayer('/video/${{encodeURIComponent(v.relative_path)}}', '${{v.name.replace(/'/g, "\\\\'")}}')">
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
        
        // 排序视频
        function sortVideos(field, order) {{
            currentSort = {{ field, order }};
            document.querySelectorAll('.sort-btn').forEach(btn => btn.classList.remove('active'));
            event.target.classList.add('active');
            renderVideos();
        }}
        
        // 过滤视频
        function filterVideos() {{
            searchTerm = document.getElementById('searchInput').value;
            renderVideos();
        }}
        
        // ESC 键关闭播放器
        document.addEventListener('keydown', function(e) {{
            if (e.key === 'Escape') closePlayer();
        }});
        
        // 页面加载时从服务器获取视频列表
        document.addEventListener('DOMContentLoaded', function() {{
            fetchVideos();
            // 每30秒自动刷新一次
            setInterval(fetchVideos, 30000);
        }});
    </script>
</body>
</html>"#, addresses)
}

/// URL 编码函数
/// 
/// # 编码规则
/// - 字母、数字、- _ . ~ 保持不变
/// - 空格编码为 %20
/// - 其他字符编码为 %XX（UTF-8 字节）
fn urlencoding_encode(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        match c {
            // 安全字符：不编码
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            // 空格：编码为 %20
            ' ' => result.push_str("%20"),
            // 其他字符：按 UTF-8 字节编码
            _ => {
                for byte in c.to_string().as_bytes() {
                    // {:02X}: 两位十六进制，不足补零
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// 格式化文件大小（未使用，保留备用）
#[allow(dead_code)]
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    // log(): 自然对数
    // powi(): 整数次幂
    let i = (bytes as f64).log(k).floor() as i32;
    format!("{:.2} {}", bytes as f64 / k.powi(i), sizes[i as usize])
}

// ============================================
// 应用入口
// ============================================

/// Tauri 应用入口函数
/// 
/// # 属性说明
/// - `#[cfg_attr(mobile, tauri::mobile_entry_point)]`:
///   在移动平台时使用 Tauri 的移动端入口点
/// 
/// # Tauri Builder
/// - `.plugin()`: 添加插件
/// - `.invoke_handler()`: 注册命令处理函数
/// - `.run()`: 启动应用
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 初始化插件
        .plugin(tauri_plugin_opener::init())    // 打开外部链接
        .plugin(tauri_plugin_dialog::init())    // 文件对话框
        .plugin(tauri_plugin_fs::init())        // 文件系统
        .plugin(tauri_plugin_shell::init())     // Shell 命令
        // 注册 Tauri 命令
        // generate_handler! 宏: 生成命令处理器
        .invoke_handler(tauri::generate_handler![
            scan_videos,
            get_shared_videos,
            play_video,
            cancel_scan,
            start_share_server,
            stop_share_server,
            get_server_status
        ])
        // 启动应用
        // generate_context! 宏: 生成应用上下文
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
