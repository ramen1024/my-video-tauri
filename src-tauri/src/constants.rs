//! 应用常量集中管理
//!
//! 本模块集中存放后端各模块使用的硬编码常量，便于统一维护和前后端对齐。

/// Session 有效时长（秒）
pub const SESSION_DURATION_SECS: i64 = 3600;

/// 最大连续登录失败次数，超过后锁定 IP
pub const MAX_FAILED_ATTEMPTS: u32 = 3;

/// IP 锁定持续时长（秒）
pub const LOCK_DURATION_SECS: i64 = 30;

/// IP 地址缓存有效期（秒）
pub const IP_CACHE_TTL_SECS: u64 = 300;

/// 最小视频文件大小（字节），小于此值的文件会被忽略
pub const MIN_VIDEO_FILE_SIZE_BYTES: u64 = 1_048_576;

/// Session 清理后台线程执行间隔（秒）
pub const SESSION_CLEANUP_INTERVAL_SECS: u64 = 600;

/// 认证请求体最大允许大小（字节）
pub const MAX_AUTH_BODY_SIZE_BYTES: u64 = 1024;

/// 支持的视频扩展名与对应 HTTP Content-Type（扩展名单一事实来源）
///
/// 扫描过滤与网页流式响应共用本表；新增格式时只需添加一项，
/// `is_supported_video_extension` 与 `video_content_type` 即同步生效。
pub const VIDEO_TYPES: &[(&str, &str)] = &[
    ("mp4", "video/mp4"),
    ("m4v", "video/mp4"),
    ("mkv", "video/x-matroska"),
    ("webm", "video/webm"),
    ("avi", "video/x-msvideo"),
    ("mov", "video/quicktime"),
    ("wmv", "video/x-ms-wmv"),
    ("flv", "video/x-flv"),
    ("mpg", "video/mpeg"),
    ("mpeg", "video/mpeg"),
];

/// 判断（小写）扩展名是否为受支持的视频格式
pub fn is_supported_video_extension(ext: &str) -> bool {
    VIDEO_TYPES.iter().any(|(known, _)| *known == ext)
}

/// 获取视频扩展名对应的 HTTP Content-Type，未知扩展名回退 application/octet-stream
pub fn video_content_type(ext: &str) -> &'static str {
    VIDEO_TYPES
        .iter()
        .find(|(known, _)| *known == ext)
        .map(|(_, mime)| *mime)
        .unwrap_or("application/octet-stream")
}

/// 视频刷新接口冷却时间（秒）
pub const REFRESH_COOLDOWN_SECS: u64 = 5;

/// 共享服务器启动超时时间（秒）
pub const SERVER_START_TIMEOUT_SECS: u64 = 10;

/// 共享服务器停止时等待单个 worker 线程退出的超时时间（秒）
pub const SERVER_STOP_TIMEOUT_SECS: u64 = 5;

/// 共享服务器 worker 线程数回退默认值
pub const SERVER_WORKER_DEFAULT_COUNT: usize = 4;

/// 共享服务器 worker 线程数上限
///
/// worker 只处理小请求（视频流在独立线程写出），核心数再多也无收益，
/// 限制上限避免高配机器创建大量空闲线程。
pub const SERVER_WORKER_MAX_COUNT: usize = 16;

/// 端口被占用时自动尝试的端口数量（从指定端口开始向后尝试）
pub const MAX_PORT_ATTEMPTS: u16 = 5;

/// 视频流式响应最大并发数
///
/// 每路视频流各占一个独立线程，超出上限的请求退回 worker 内同步写出，
/// 避免并发观看人数过多时线程数无限膨胀。
pub const MAX_CONCURRENT_STREAMS: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_types_has_no_duplicate_extensions() {
        let mut exts: Vec<&str> = VIDEO_TYPES.iter().map(|(e, _)| *e).collect();
        exts.sort_unstable();
        let total = exts.len();
        exts.dedup();
        assert_eq!(exts.len(), total, "映射表中的扩展名不应重复");
    }

    #[test]
    fn test_extension_support_and_content_type() {
        assert!(is_supported_video_extension("mp4"));
        assert!(is_supported_video_extension("mpeg"));
        assert!(!is_supported_video_extension("txt"));
        // 大小写归一化由调用方负责，本函数仅接受小写输入
        assert!(!is_supported_video_extension("MP4"));

        assert_eq!(video_content_type("mkv"), "video/x-matroska");
        assert_eq!(video_content_type("m4v"), "video/mp4");
        assert_eq!(video_content_type(""), "application/octet-stream");
        assert_eq!(
            video_content_type("does-not-exist"),
            "application/octet-stream"
        );
    }
}
