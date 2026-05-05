//! Tauri IPC 命令模块
//!
//! 将后端功能封装为 Tauri 命令，供前端通过 invoke() 调用。
//! 每个子模块对应一个功能领域：
//! - `video`: 视频扫描与管理
//! - `share`: 局域网共享服务器控制
//! - `password_cmd`: 密码保护管理

pub mod video;
pub mod share;
pub mod password_cmd;
