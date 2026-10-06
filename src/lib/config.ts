/**
 * 应用配置常量
 *
 * 与后端 `src-tauri/src/constants.rs` 一一对应。**这不是可以随手改的文件**：
 * 每一项都被 Rust 侧的跨文件一致性测试（`src-tauri/src/server/csp_tests.rs`）
 * 断言过，改这里不改那边会让 `cargo test` 红灯。
 */

/** 默认局域网共享服务器端口（对应 Rust `constants::DEFAULT_SHARE_PORT`） */
export const DEFAULT_SHARE_PORT = 6008;

/**
 * 最小视频文件大小（字节），小于此值的文件在扫描时被跳过
 *
 * 对应 Rust `constants::MIN_VIDEO_FILE_SIZE_BYTES`。前端只用它来生成
 * "已跳过 N 个过小文件（小于 …）" 的提示文案。
 */
export const MIN_VIDEO_FILE_SIZE_BYTES = 1_048_576;
