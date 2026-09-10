//! 工具函数模块
//!
//! 提供路径验证、IP 检测、URL 解码、ETag 计算等通用工具函数。

use std::path::Path;
use std::sync::LazyLock;
use std::time::SystemTime;

use parking_lot::RwLock;
use sha2::{Digest, Sha256};

use crate::constants::IP_CACHE_TTL_SECS;
use crate::models::VideoFile;

/// IP 地址缓存类型：(IP列表, 缓存时间)
type IpCacheValue = (Vec<String>, std::time::Instant);

/// IP 地址缓存，存储 (IP列表, 缓存时间)
static CACHED_IPS: LazyLock<RwLock<Option<IpCacheValue>>> = LazyLock::new(|| RwLock::new(None));

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
            if cached_at.elapsed() < std::time::Duration::from_secs(IP_CACHE_TTL_SECS) {
                return ips.clone();
            }
        }
    }

    let ips = detect_local_ips();
    *CACHED_IPS.write() = Some((ips.clone(), std::time::Instant::now()));
    ips
}

/// 实际检测本机 IP 地址（通过 if-addrs crate 调用系统 API）
///
/// 仅保留全局 IPv4 地址：过滤回环、IPv6 与链路本地地址
/// （虚拟机/虚拟专用网络适配器的 IPv6 与 169.254.x.x 对局域网访问无意义）。
/// 如果获取失败或没有可用地址，则回退到 127.0.0.1。
fn detect_local_ips() -> Vec<String> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            let ips: Vec<String> = interfaces
                .into_iter()
                .filter_map(|iface| {
                    let ip = iface.addr.ip();
                    // 仅保留全局 IPv4（过滤回环地址与 IPv6）
                    if ip.is_loopback() || !ip.is_ipv4() {
                        return None;
                    }
                    let ip_str = ip.to_string();
                    // APIPA 链路本地地址（169.254.0.0/16），局域网访问不可用
                    if ip_str.starts_with("169.254.") {
                        return None;
                    }
                    Some(ip_str)
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

/// 将共享文件夹加入 asset 协议访问范围（Tauri 2 运行时 API）
///
/// 配置文件中的 `assetProtocol.scope` 保持最小化（空），用户选择文件夹后在此
/// 动态放行，避免 `**` 全盘范围带来的任意文件读取风险。失败仅记日志，
/// 桌面端仍可回退使用系统播放器打开视频。
///
/// 只放行、不撤销：切换文件夹后旧目录仍在放行范围内。范围里都是用户显式选择过的
/// 目录，风险可控；若要收紧为"仅当前目录"，需配合 `Scope::forbid_directory`，
/// 但 forbid 的优先级高于 allow 且不会因再次 allow 而解除，切换回旧目录会被误伤。
pub fn allow_shared_folder_asset_scope(app: &tauri::AppHandle, folder_path: &str) {
    use tauri::Manager;
    if let Err(e) = app
        .asset_protocol_scope()
        .allow_directory(folder_path, true)
    {
        log::warn!("[共享] 设置 asset 协议访问范围失败: {}", e);
    }
}

/// URL 解码函数，使用标准 percent-encoding 库
pub fn urlencoding_decode(input: &str) -> String {
    percent_encoding::percent_decode_str(input)
        .decode_utf8_lossy()
        .to_string()
}

/// 为视频列表计算稳定 ETag（SHA-256 十六进制，不含引号）
///
/// 基于全部视频的 (相对路径, 大小, 修改时间) 计算指纹，任意文件的新增、删除、改名、
/// 大小或修改时间变化都会导致 ETag 变化，避免只取首尾文件时中间文件变更产生碰撞、
/// 导致网页端缓存一直显示陈旧列表。
///
/// 各字段之间以 0xFF 分隔，避免相邻字段字节拼接产生歧义。
/// 计算仅借用数据，不产生字符串克隆。
pub fn compute_videos_etag(videos: &[VideoFile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(videos.len().to_be_bytes());
    for v in videos {
        hasher.update(v.relative_path.as_bytes());
        hasher.update([0xFF]);
        hasher.update(v.size.to_be_bytes());
        hasher.update([0xFF]);
        hasher.update(v.modified.as_deref().unwrap_or("").as_bytes());
        hasher.update([0xFF]);
    }
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
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
            canonical_path,
            canonical_base
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{make_temp_dir, sample_video};
    use std::fs;

    #[test]
    fn test_sanitize_video_path_traversal() {
        let base = make_temp_dir("sanitize_traversal");
        let result = sanitize_video_path(&base, "../../etc/passwd");
        assert!(result.is_none(), "路径遍历攻击应被阻止");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_sanitize_video_path_valid() {
        let base = make_temp_dir("sanitize_valid");
        let file_path = base.join("movie.mp4");
        fs::File::create(&file_path).expect("创建测试文件失败");

        let result = sanitize_video_path(&base, "movie.mp4");
        assert!(result.is_some(), "合法路径应被允许");
        assert_eq!(
            result.unwrap(),
            file_path.canonicalize().unwrap(),
            "返回的路径应与规范路径一致"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_is_root_directory() {
        assert!(
            is_root_directory(Path::new("C:\\")),
            "Windows 根目录应被识别"
        );
        assert!(is_root_directory(Path::new("/")), "Unix 根目录应被识别");
        assert!(
            !is_root_directory(Path::new("C:\\Users")),
            "普通 Windows 子目录不应是根目录"
        );
        assert!(
            !is_root_directory(Path::new("/home")),
            "普通 Unix 子目录不应是根目录"
        );
    }

    #[test]
    fn test_compute_videos_etag_is_stable() {
        let videos = vec![
            sample_video("a.mp4", 1024, Some("2024-01-01 10:00:00")),
            sample_video("b.mp4", 2048, Some("2024-01-02 10:00:00")),
        ];
        let etag1 = compute_videos_etag(&videos);
        let etag2 = compute_videos_etag(&videos);
        assert_eq!(etag1, etag2, "同一视频列表生成的 ETag 应保持一致");
        assert_eq!(etag1.len(), 64, "SHA-256 十六进制输出长度应为 64");
    }

    #[test]
    fn test_compute_videos_etag_differs_for_different_lists() {
        let videos_a = vec![sample_video("a.mp4", 1024, None)];
        let videos_b = vec![sample_video("a.mp4", 2048, None)];
        assert_ne!(
            compute_videos_etag(&videos_a),
            compute_videos_etag(&videos_b),
            "文件大小变化应生成不同 ETag"
        );
    }

    #[test]
    fn test_compute_videos_etag_changes_when_middle_file_renamed() {
        // 回归测试：中间文件改名（首尾与总数不变）也必须改变 ETag，
        // 否则网页端会因命中 304 一直显示陈旧列表。
        let videos_before = vec![
            sample_video("a.mp4", 1024, None),
            sample_video("b.mp4", 2048, None),
            sample_video("c.mp4", 4096, None),
        ];
        let videos_after = vec![
            sample_video("a.mp4", 1024, None),
            sample_video("bb.mp4", 2048, None),
            sample_video("c.mp4", 4096, None),
        ];
        assert_ne!(
            compute_videos_etag(&videos_before),
            compute_videos_etag(&videos_after),
            "中间文件改名应生成不同 ETag"
        );
    }

    #[test]
    fn test_compute_videos_etag_changes_when_modified_time_changes() {
        let videos_before = vec![sample_video("a.mp4", 1024, Some("2024-01-01 10:00:00"))];
        let videos_after = vec![sample_video("a.mp4", 1024, Some("2024-06-01 10:00:00"))];
        assert_ne!(
            compute_videos_etag(&videos_before),
            compute_videos_etag(&videos_after),
            "修改时间变化应生成不同 ETag"
        );
    }
}
