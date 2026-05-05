use std::path::Path;
use std::sync::LazyLock;
use std::time::SystemTime;

use parking_lot::RwLock;

static CACHED_IPS: LazyLock<RwLock<Option<(Vec<String>, std::time::Instant)>>> =
    LazyLock::new(|| RwLock::new(None));

const IP_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

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
/// 通过 if-addrs crate 直接调用系统 API 获取网络接口地址
///
/// # 返回
/// IP 地址列表，过滤掉回环地址
pub fn get_local_ips() -> Vec<String> {
    {
        let cache = CACHED_IPS.read();
        if let Some((ref ips, cached_at)) = *cache {
            if cached_at.elapsed() < IP_CACHE_TTL {
                return ips.clone();
            }
        }
    }

    let ips = detect_local_ips();
    *CACHED_IPS.write() = Some((ips.clone(), std::time::Instant::now()));
    ips
}

fn detect_local_ips() -> Vec<String> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            let ips: Vec<String> = interfaces
                .into_iter()
                .filter_map(|iface| {
                    if iface.is_loopback() {
                        None
                    } else {
                        Some(iface.addr.ip().to_string())
                    }
                })
                .collect();
            if ips.is_empty() {
                vec!["127.0.0.1".to_string()]
            } else {
                ips
            }
        }
        Err(e) => {
            log::warn!("[网络] 获取本机IP失败: {}", e);
            vec!["127.0.0.1".to_string()]
        }
    }
}

/// URL 解码函数，使用标准 percent-encoding 库
pub fn urlencoding_decode(input: &str) -> String {
    percent_encoding::percent_decode_str(input)
        .decode_utf8_lossy()
        .to_string()
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
        log::warn!(
            "Path traversal blocked: {:?} is outside {:?}",
            canonical_path, canonical_base
        );
        None
    }
}
