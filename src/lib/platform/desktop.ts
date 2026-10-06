/**
 * 桌面端（Tauri webview）后端实现
 *
 * 所有 IPC 调用都经由 `$lib/services/*`，本文件只负责把 IPC 的数据形状
 * 映射到应用统一的 [`VideoItem`]，并补齐平台能力标志。
 */

import { open } from "@tauri-apps/plugin-dialog";
import {
  cancelScan as cancelScanIpc,
  getSharedVideos,
  getVideoSrc,
  playVideo as playVideoIpc,
  scanVideos,
} from "$lib/services/video";
import { isSupportedFormat } from "$lib/utils/format";
import type { VideoFile } from "$lib/types";
import type { Platform, VideoItem } from "./types";

/** Rust `VideoFile` → 应用内统一视频条目 */
export function toVideoItem(video: VideoFile): VideoItem {
  return {
    name: video.name,
    relativePath: video.relative_path,
    size: video.size,
    modified: video.modified,
    extension: video.extension,
    path: video.path,
  };
}

export const desktop: Platform = {
  kind: "desktop",
  canPickFolder: true,
  canShare: true,
  listPollIntervalMs: null,

  async pickFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择视频文件夹",
    });
    return typeof selected === "string" ? selected : null;
  },

  async rescan(folder) {
    const videos = await scanVideos(folder);
    return videos.map(toVideoItem);
  },

  async loadVideos() {
    const videos = await getSharedVideos();
    return videos.map(toVideoItem);
  },

  async cancelScan() {
    await cancelScanIpc();
  },

  canPlayInline(video) {
    // webview 只原生支持 mp4/webm/m4v，其余格式交给系统播放器
    return isSupportedFormat(video.extension);
  },

  videoSrc(video) {
    return getVideoSrc(video.path ?? "");
  },

  async openWithSystemPlayer(video) {
    await playVideoIpc(video.path ?? "");
  },
};
