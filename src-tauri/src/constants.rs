//! 应用常量集中管理
//!
//! 本模块集中存放后端各模块使用的硬编码常量，便于统一维护和前后端对齐。

/// Session 有效时长（秒）
///
/// 取值 1 小时：局域网观影的典型时长，期间不必反复输密码；到期后重新认证。
/// 目标是"够用一天中的一次使用场景"，而不是长期免登录。
pub const SESSION_DURATION_SECS: i64 = 3600;

/// 最大连续登录失败次数，超过后锁定 IP
///
/// 与 4 位数字密码（10000 种组合）配合使用：3 次失败即锁定，
/// 使在线穷举在时间上不可行（详见 `password.rs` 的 `set_password` 注释）。
pub const MAX_FAILED_ATTEMPTS: u32 = 3;

/// IP 锁定持续时长（秒）
///
/// 取值 30 秒：对真人足够长（足以劝退手滑或好奇的尝试），
/// 对暴力破解足够短到"每 30 秒只能试 3 次"——穷举 10000 种组合需数天。
/// 再长会让输错密码的家庭成员等得烦躁，再短则失去意义。
pub const LOCK_DURATION_SECS: i64 = 30;

/// IP 地址缓存有效期（秒）
pub const IP_CACHE_TTL_SECS: u64 = 300;

/// 最小视频文件大小（字节），小于此值的文件会被跳过（但**不会被静默丢弃**）
///
/// 取值 1 MiB：目标是过滤掉 `.mp4` 之类的空壳文件、缩略图缓存、以及下载中断留下的
/// 残缺文件——这类文件出现在列表里只会让用户点了播不出来。
///
/// 关键在于"跳过"必须**可见**：被跳过的文件会连同文件名与大小一起记录进
/// `ScanReport::skipped_small`，由 `scan_videos` 的返回值与 `/refresh-status`
/// 一并交给界面提示。此前的实现是直接 `return None`，用户只看到"我明明放了 20 个
/// 视频，列表里只有 18 个"，却无从得知原因。
pub const MIN_VIDEO_FILE_SIZE_BYTES: u64 = 1_048_576;

/// 单次扫描最多在报告中列出多少个被跳过的文件
///
/// 扫描一个含上千个小文件的目录时，报告本身不该变成新的问题源；超出部分只计数、
/// 不列名（`ScanReport::skipped_small_truncated` 标记是否被截断）。
pub const MAX_SKIPPED_FILES_REPORTED: usize = 20;

/// Session 清理后台线程执行间隔（秒）
pub const SESSION_CLEANUP_INTERVAL_SECS: u64 = 600;

/// 认证请求体最大允许大小（字节）
pub const MAX_AUTH_BODY_SIZE_BYTES: u64 = 1024;

/// 默认局域网共享端口
///
/// 前端 `src/lib/config.ts` 的 `DEFAULT_SHARE_PORT` 必须与此一致（有测试断言）。
pub const DEFAULT_SHARE_PORT: u16 = 6008;

/// Vite 开发服务器端口（`vite.config.js` 的 `server.port`）
///
/// 仅用于 Tauri 导航守卫放行 dev server；生产构建不会用到。
pub const DEV_SERVER_PORT: u16 = 1420;

/// Vite HMR 端口（`vite.config.js` 的 `server.hmr.port`）
pub const DEV_HMR_PORT: u16 = 1421;

/// 浏览器侧（内嵌 HTTP 服务器）CSP 中 `media-src` 的取值
///
/// 与 `tauri.conf.json` 的差异是**有意**的：桌面端通过 `asset:` 协议播放本机文件，
/// 浏览器侧走同源 `/video/*`，且不允许 `asset:`（浏览器没有这个协议）。
pub const WEB_CSP_MEDIA_SRC: &str = "media-src 'self' blob:";

/// 浏览器侧（内嵌 HTTP 服务器）CSP 中 `img-src` 的取值
///
/// 同上：不含 `asset:` 与 `https://asset.localhost`。
pub const WEB_CSP_IMG_SRC: &str = "img-src 'self' data:";

/// 两侧 CSP 必须一致的指令（指令名 → 取值）
///
/// 这两条在两侧都是**显式写出**的，因此可以直接逐字比对：改了一侧忘了另一侧，
/// `csp_directives_written_on_both_sides_match` 会立刻红灯。
///
/// 为什么只有两条：`tauri.conf.json` 刻意只显式声明"本机需要放行/收紧的"指令，
/// 其余（`script-src` / `connect-src` / `object-src` / `base-uri` /
/// `frame-ancestors`）**依赖 `default-src 'self'` 兜底**——这是 Tauri 的常见写法，
/// 因为 `tauri-codegen` 会在构建期为内联脚本往 `script-src` 追加 sha256 哈希，
/// 预先写一条显式的 `script-src` 反而会让注入语义更难读。浏览器侧没有 codegen，
/// 只能把这些指令全部显式写出。因此"指令集合相同"不成立，真正的约束是：
/// **浏览器侧显式收紧的每一条，都必须不比桌面端经由 `default-src` 得到的策略更松**，
/// 由 `web_csp_is_no_looser_than_default_src` 断言。
pub const CSP_SHARED_DIRECTIVES: &[(&str, &str)] = &[
    ("default-src", "'self'"),
    ("style-src", "'self' 'unsafe-inline'"),
];

/// 浏览器侧显式声明、其取值必须与 `'self'` 同等或更紧的指令
///
/// 这些指令在 `tauri.conf.json` 里没有显式写出（回落到 `default-src 'self'`），
/// 但浏览器侧必须写出来。取值只能是 `'self'` 或 `'none'`——出现任何额外来源
/// （`*`、`http:`、`unsafe-inline` 等）都意味着浏览器侧比桌面端更松，
/// 属于"改了 CSP 却悄悄放宽了浏览器端"的回归。
///
/// 本表没有运行期消费者：`frontend_csp` 里这些取值是直接写死的，本表只作为
/// "这些指令必须存在且不得放宽"的声明，由 `csp_tests` 断言。因此标注
/// `#[cfg(test)]`，避免在发布产物里留下一条无人使用的常量。
#[cfg(test)]
pub const WEB_CSP_HARDENED_DIRECTIVES: &[&str] = &[
    "script-src",
    "connect-src",
    "object-src",
    "base-uri",
    "frame-ancestors",
];

/// 支持的视频扩展名与对应 HTTP Content-Type（扩展名单一事实来源）
///
/// 扫描过滤与网页流式响应共用本表；新增格式时只需添加一项，
/// `is_supported_video_extension` 与 `video_content_type` 即同步生效。
///
/// **这是"可扫描"的准入清单，不是"可播放"的保证**：列在这里只表示该扩展名
/// 会被扫描进列表、并按下面的 Content-Type 提供给浏览器，是否能真正解码取决于
/// 播放端的容器/编码支持（见 `src/lib/utils/format.ts` 的 `INLINE_PLAYABLE_EXTENSIONS`）。
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

    /// 所有扩展名都必须是纯小写字母数字
    ///
    /// 比较前统一 `to_lowercase()`，因此写大写必然匹配不上；同时避免出现
    /// `"mp4 "`（尾随空格）、`".mp4"`（带点）这类肉眼不易发现的错误。
    #[test]
    fn test_video_types_are_lowercase_bare_extensions() {
        for (ext, mime) in VIDEO_TYPES {
            assert_eq!(
                *ext,
                ext.to_lowercase(),
                "扩展名 {ext:?} 应为小写（比较前会 to_lowercase，写大写匹配不上）"
            );
            assert!(
                !ext.starts_with('.'),
                "扩展名 {ext:?} 不应带前导点（Path::extension 不返回点）"
            );
            assert!(
                ext.chars().all(|c| c.is_ascii_alphanumeric()),
                "扩展名 {ext:?} 应只含 ASCII 字母数字"
            );
            assert!(
                mime.starts_with("video/"),
                "{ext:?} 的 Content-Type {mime:?} 应为 video/* 类型"
            );
        }
    }

    /// `VIDEO_TYPES` 是"可扫描"准入清单，本测试锁定其中**有意排除**的格式
    ///
    /// 作用是把一个隐式决定变成显式断言：`rmvb` / `3gp` / `ogv` 等格式本应用完全不处理
    /// （既不做为可扫描格式，也不在前端内联播放清单里），若将来有人"顺手加进来"，
    /// 必须同时想清楚前端该怎么播放它们。
    ///
    /// 注意这里**不含** `avi` / `wmv` / `flv` / `mpg`：它们虽不能被内置播放器解码，
    /// 但作为"可扫描 + 交给系统播放器"的格式是**有意保留**的（见
    /// `src/lib/utils/format.ts` 的 `INLINE_PLAYABLE_EXTENSIONS` 说明），
    /// 因此不属于"应排除"。
    #[test]
    fn test_unhandled_containers_are_excluded_from_scan() {
        for ext in ["rmvb", "3gp", "ogv", "ts", "m2ts", "vob", "asf"] {
            assert!(
                !is_supported_video_extension(ext),
                "{ext:?} 目前不在本应用的处理范围内，若确要支持需同时安排前端播放方式"
            );
        }
    }

    /// 不可内联播放但**有意保留在扫描清单**里的格式不得被误删
    ///
    /// 与上一条互为补充：这些格式的价值是"能被扫描出来并交给系统播放器"。
    /// 若有人把"vi 不能被 `<video>` 解码"误当成"不该被扫描"而删掉它们，
    /// 用户就彻底看不到这些文件了——比点了播不出来更糟。
    #[test]
    fn test_system_player_only_containers_stay_scannable() {
        for ext in ["avi", "wmv", "flv", "mpg", "mpeg"] {
            assert!(
                is_supported_video_extension(ext),
                "{ext:?} 应保留在扫描清单中：内置播放器放不出来，但仍可通过系统播放器打开"
            );
        }
    }
}
