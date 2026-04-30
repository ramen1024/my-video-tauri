use std::path::Path;
use std::time::SystemTime;

/// 将系统时间格式化为可读字符串
///
/// # 参数
/// - `time`: SystemTime 类型的时间
///
/// # 返回
/// 格式化后的字符串，如 "2024-01-15 14:30:00"
pub fn format_system_time(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = time.into();
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
pub fn is_root_directory(path: &Path) -> bool {
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
pub fn get_local_ips() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("cmd")
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
        // 非 Windows 平台暂返回默认值
        vec!["127.0.0.1".to_string()]
    }
}

/// URL 解码函数
///
/// # 实现原理
/// 1. 遇到 %XX 时，将 XX 解析为十六进制字节
/// 2. 收集所有字节后，转换为 UTF-8 字符串
pub fn urlencoding_decode(input: &str) -> String {
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

/// 验证请求路径是否在允许的目录范围内，防止路径遍历攻击
///
/// # 安全检查
/// - 解析并规范化路径（去除 .. 等组件）
/// - 确保最终路径仍在 base 目录内
/// - 返回 None 表示路径不合法
pub fn sanitize_video_path(base: &Path, requested: &str) -> Option<std::path::PathBuf> {
    let decoded = urlencoding_decode(requested);
    let normalized = decoded.replace('/', std::path::MAIN_SEPARATOR_STR);

    let joined = base.join(&normalized);

    let canonical_path = joined.canonicalize().ok()?;
    let canonical_base = base.canonicalize().ok()?;

    if canonical_path.starts_with(&canonical_base) {
        Some(canonical_path)
    } else {
        println!(
            "Path traversal blocked: {:?} is outside {:?}",
            canonical_path, canonical_base
        );
        None
    }
}

/// 格式化文件大小
#[allow(dead_code)]
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as i32;
    format!("{:.2} {}", bytes as f64 / k.powi(i), sizes[i as usize])
}
