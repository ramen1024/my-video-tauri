/**
 * 类型定义模块
 *
 * 定义前后端共享的 TypeScript 接口和类型，
 * 与 Rust 后端的 models.rs / error.rs 中的结构体一一对应。
 */

/** 视频文件信息，对应 Rust VideoFile */
export interface VideoFile {
  /** 文件名（不含路径） */
  name: string;
  /** 完整绝对路径 */
  path: string;
  /** 相对于扫描目录的相对路径（用于网页端访问） */
  relative_path: string;
  /** 文件大小（字节） */
  size: number;
  /** 修改时间，可能为 null */
  modified: string | null;
  /** 文件扩展名（小写） */
  extension: string;
}

/** 共享服务器信息，对应 Rust ShareServerInfo */
export interface ShareServerInfo {
  /** 本机所有可用 IP 地址 */
  ips: string[];
  /** HTTP 服务器监听端口 */
  port: number;
}

/** 一次扫描中被跳过的文件，对应 Rust SkippedFile */
export interface SkippedFile {
  /** 文件名（不含路径） */
  name: string;
  /** 文件大小（字节） */
  size: number;
}

/** 扫描结果摘要，对应 Rust ScanReport */
export interface ScanReport {
  /** 成功纳入列表的视频数量 */
  total: number;
  /** 因小于最小体积而跳过的文件（最多列出前 N 个） */
  skipped_small: SkippedFile[];
  /** 因小于最小体积而跳过的文件总数（可能大于 skipped_small.length） */
  skipped_small_count: number;
  /** skipped_small 是否只列出了部分条目 */
  skipped_small_truncated: boolean;
}

/** 扫描结果：视频列表 + 本次扫描报告 */
export interface ScanResult {
  videos: VideoFile[];
  report: ScanReport;
}

/** 共享服务器状态，对应 Rust ShareStatus（webview 重载后恢复界面用） */
export interface ShareStatus {
  /** 服务器是否正在运行 */
  running: boolean;
  /** 运行时的本机 IP 列表，未运行时为空数组 */
  ips: string[];
  /** 运行时的监听端口，未运行时为 0 */
  port: number;
  /** 当前共享（上次扫描）的文件夹路径，未设置时为空字符串 */
  folder_path: string;
}

/** 密码保护状态，对应 Rust PasswordStatus */
export interface PasswordStatus {
  /** 密码保护是否已启用 */
  enabled: boolean;
  /** 是否已设置密码 */
  has_password: boolean;
}

/** 排序字段类型 */
export type SortField = "name" | "size" | "modified";
/** 排序方向 */
export type SortDirection = "asc" | "desc";

/** 结构化错误类型，对应 Rust AppError 的 serde 序列化格式 */
export interface AppError {
  type: "InvalidPath" | "ScanCancelled" | "ServerAlreadyRunning" | "ServerNotRunning" | "IoError" | "PasswordError" | "Other";
  message: string;
}

/**
 * 从 Tauri IPC 错误中提取可读的错误信息
 *
 * Tauri 返回的错误可能是结构化的 AppError 或未知类型，此函数统一处理
 */
/** 已知错误类型 → 中文文案兜底（后端正常情况下会携带 message，此表仅作防御） */
const APP_ERROR_TYPE_MESSAGES: Record<string, string> = {
  ScanCancelled: "扫描已取消",
  ServerAlreadyRunning: "服务器已在运行",
  ServerNotRunning: "服务器未运行",
};

export function parseAppError(e: unknown): string {
  if (typeof e === "object" && e !== null) {
    const err = e as Record<string, unknown>;
    if (typeof err.type === "string") {
      if (typeof err.message === "string" && err.message.length > 0) {
        return err.message;
      }
      return APP_ERROR_TYPE_MESSAGES[err.type] ?? err.type;
    }
  }
  return String(e);
}
