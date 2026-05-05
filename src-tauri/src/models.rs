//! 数据模型定义
//!
//! 定义视频文件和共享服务器信息的结构体，用于 Tauri IPC 通信和 HTTP API 响应。

use serde::Serialize;

/// 视频文件信息结构体
#[derive(Debug, Serialize, Clone)]
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
#[derive(Debug, Serialize, Clone)]
pub struct ShareServerInfo {
    pub ips: Vec<String>,
    pub port: u16,
}
