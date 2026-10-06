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

/// 一次扫描中被跳过的文件（仅记录原因，不含绝对路径）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkippedFile {
    /// 文件名（不含路径）
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
}

/// 一次扫描的结果摘要
///
/// 用于回答"为什么列表里少了我放进去的文件"：扫描不再是"静默过滤"，
/// 而是把每个被跳过的文件连同原因带回给界面。
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ScanReport {
    /// 成功纳入列表的视频数量
    pub total: usize,
    /// 因小于 [`crate::constants::MIN_VIDEO_FILE_SIZE_BYTES`] 而跳过的文件
    pub skipped_small: Vec<SkippedFile>,
    /// 因小于阈值而跳过的文件总数（可能大于 `skipped_small.len()`）
    pub skipped_small_count: usize,
    /// `skipped_small` 是否只列出了部分条目（总数超出上限）
    pub skipped_small_truncated: bool,
}

impl ScanReport {
    /// 记录一个因过小而跳过的文件
    ///
    /// 超出 [`crate::constants::MAX_SKIPPED_FILES_REPORTED`] 时只累加计数并标记截断，
    /// 避免"小文件极多"的目录把报告本身撑爆。
    pub fn record_skipped_small(&mut self, name: String, size: u64) {
        self.skipped_small_count += 1;
        if self.skipped_small.len() < crate::constants::MAX_SKIPPED_FILES_REPORTED {
            self.skipped_small.push(SkippedFile { name, size });
        } else {
            self.skipped_small_truncated = true;
        }
    }

    /// 是否存在任何被跳过的文件
    pub fn has_skipped(&self) -> bool {
        self.skipped_small_count > 0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `ScanReport` 的 JSON 形状必须与前端 `src/lib/types/index.ts` 的
    /// `ScanReport` / `SkippedFile` 接口逐字对应
    ///
    /// 这是 IPC 契约：Rust 侧改字段名（例如 `skipped_small` → `skipped`）而没同步
    /// TS 接口时，`pnpm check` 不会报错（TS 接口是手写的，不来自 Rust），
    /// 界面只会静默显示不出"被跳过的文件"。此处用序列化结果把契约锁死。
    #[test]
    fn scan_report_serializes_with_expected_field_names() {
        let mut report = ScanReport {
            total: 3,
            ..Default::default()
        };
        report.record_skipped_small("a.mp4".to_string(), 100);

        let json = serde_json::to_string(&report).expect("应可序列化");

        for expected in [
            "\"total\":3",
            "\"skipped_small\":[",
            "\"skipped_small_count\":1",
            "\"skipped_small_truncated\":false",
            "\"name\":\"a.mp4\"",
            "\"size\":100",
        ] {
            assert!(
                json.contains(expected),
                "ScanReport 的 JSON 应包含 {expected}，实际：{json}"
            );
        }
    }

    /// 截断上限之外只累加计数，不再追加明细
    #[test]
    fn record_skipped_small_truncates_details_but_counts_all() {
        let limit = crate::constants::MAX_SKIPPED_FILES_REPORTED;
        let mut report = ScanReport::default();
        for i in 0..limit + 8 {
            report.record_skipped_small(format!("f{i}.mp4"), 1);
        }

        assert_eq!(report.skipped_small.len(), limit, "明细不应超过上限");
        assert_eq!(
            report.skipped_small_count,
            limit + 8,
            "计数必须完整，不受明细截断影响"
        );
        assert!(report.skipped_small_truncated, "截断后必须置标记");
    }
}
