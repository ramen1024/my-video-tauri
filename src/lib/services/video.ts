import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { VideoFile } from "$lib/types";

export async function scanVideos(folderPath: string): Promise<void> {
  await invoke("scan_videos", { folderPath });
}

export async function getSharedVideos(): Promise<VideoFile[]> {
  return await invoke("get_shared_videos");
}

export async function playVideo(filePath: string): Promise<void> {
  await invoke("play_video", { filePath });
}

export async function cancelScan(): Promise<void> {
  await invoke("cancel_scan");
}

export function getVideoSrc(videoPath: string): string {
  const normalizedPath = videoPath.replace(/\\/g, "/");
  return convertFileSrc(normalizedPath);
}
