//! 视频扫描器 - 应用程序入口
//!
//! 负责初始化日志系统并启动 Tauri 应用。
//! Release 模式下隐藏 Windows 控制台窗口（日志同时写入应用数据目录下的文件）。

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    video_scanner_lib::init_logging();
    video_scanner_lib::run()
}
