/**
 * 格式化工具模块
 *
 * 提供文件大小格式化和视频格式判断等工具函数。
 */

/**
 * 浏览器 <video> 标签原生支持的视频格式
 *
 * 与后端 `constants.rs` 的 `VIDEO_TYPES`（可扫描的后端扩展名清单）是两件事：
 * 后端决定"哪些文件算视频"，这里决定"哪些能直接在 webview 里播放"，
 * 其余格式走系统默认播放器（`play_video`）。两者不必也不应该对齐。
 */
const SUPPORTED_EXTENSIONS = ["mp4", "webm", "m4v"];

/** 判断视频格式是否可由浏览器原生播放 */
export function isSupportedFormat(ext: string): boolean {
  return SUPPORTED_EXTENSIONS.includes(ext.toLowerCase());
}

/**
 * 将字节数格式化为人类可读的文件大小字符串
 *
 * 示例: 1536 → "1.5 KB", 1073741824 → "1 GB"
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  // 超出最大单位时按最大单位显示，避免数组越界返回 undefined
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}
