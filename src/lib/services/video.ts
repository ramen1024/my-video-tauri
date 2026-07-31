/**
 * 视频服务模块
 *
 * 封装视频相关的 Tauri IPC 调用，包括扫描、播放和取消扫描。
 */

import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { VideoFile } from "$lib/types";

/** 扫描指定文件夹中的视频文件 */
export async function scanVideos(
  folderPath: string,
  useCache: boolean = true
): Promise<VideoFile[]> {
  return await invoke("scan_videos", { folderPath, useCache });
}

/** 使用系统默认播放器打开视频文件 */
export async function playVideo(filePath: string): Promise<void> {
  await invoke("play_video", { filePath });
}

/** 取消正在进行的视频扫描 */
export async function cancelScan(): Promise<void> {
  await invoke("cancel_scan");
}

/**
 * 将视频文件路径转换为 Tauri 可访问的资源 URL
 *
 * Windows 路径使用反斜杠，需要先转换为正斜杠再调用 convertFileSrc
 */
export function getVideoSrc(videoPath: string): string {
  const normalizedPath = videoPath.replace(/\\/g, "/");
  return convertFileSrc(normalizedPath);
}
