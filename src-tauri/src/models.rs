//! 数据模型定义
//!
//! 定义视频文件和共享服务器信息的结构体，用于 Tauri IPC 通信和 HTTP API 响应。

use serde::{Deserialize, Serialize};

/// 视频文件信息结构体
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

/// 视频文件摘要，仅用于 HTTP API 响应
///
/// 与 [`VideoFile`] 的唯一区别是不含 `path`（本机绝对路径）：
/// 局域网客户端用相对路径即可播放，不应获知服务器端的文件系统结构，
/// 因此网页端拿到的列表必须走这个结构体，而非直接序列化 `VideoFile`。
#[derive(Debug, Serialize, Clone)]
pub struct VideoSummary {
    /// 文件名（不含路径）
    pub name: String,
    /// 相对于扫描目录的相对路径（用于网页端访问）
    pub relative_path: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（可选，可能获取失败）
    pub modified: Option<String>,
    /// 文件扩展名（小写）
    pub extension: String,
}

impl From<&VideoFile> for VideoSummary {
    fn from(video: &VideoFile) -> Self {
        Self {
            name: video.name.clone(),
            relative_path: video.relative_path.clone(),
            size: video.size,
            modified: video.modified.clone(),
            extension: video.extension.clone(),
        }
    }
}

/// 共享服务器信息结构体
/// 返回给前端，包含服务器地址和视频列表
#[derive(Debug, Serialize, Clone)]
pub struct ShareServerInfo {
    pub ips: Vec<String>,
    pub port: u16,
}

/// 共享服务器状态，桌面端用于恢复界面（例如 webview 重载后）
#[derive(Debug, Serialize, Clone)]
pub struct ShareStatus {
    /// 服务器是否正在运行
    pub running: bool,
    /// 运行时的本机 IP 列表，未运行时为空
    pub ips: Vec<String>,
    /// 运行时的监听端口，未运行时为 0
    pub port: u16,
    /// 当前共享（上次扫描）的文件夹路径，未设置时为空字符串
    pub folder_path: String,
}
