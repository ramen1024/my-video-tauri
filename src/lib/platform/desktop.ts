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
import {
  getShareStatus as getShareStatusIpc,
  startShareServer,
  stopShareServer,
} from "$lib/services/share";
import { getPasswordStatus as getPasswordStatusIpc } from "$lib/services/password";
import { isInlinePlayableContainer } from "$lib/utils/format";
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
  canCancelScan: true,
  canOpenWithSystemPlayer: true,
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
    const report = await scanVideos(folder);
    // 扫描结果已写入后端状态，再取一次保证与后端一致（含排序生效后的顺序）
    const videos = await getSharedVideos();
    return { videos: videos.map(toVideoItem), report };
  },

  async loadVideos() {
    const videos = await getSharedVideos();
    return videos.map(toVideoItem);
  },

  async cancelScan() {
    await cancelScanIpc();
  },

  preferInlinePlayback(video) {
    // 清单只覆盖"实测可解码"的容器，其余（avi/wmv/flv/mpg/mpeg）直接走系统播放器；
    // 清单内的若实际放不出来（如 HEVC 视频轨、AC3 音轨），由页面回退到系统播放器
    return isInlinePlayableContainer(video.extension);
  },

  videoSrc(video) {
    return getVideoSrc(video.path ?? "");
  },

  async openWithSystemPlayer(video) {
    await playVideoIpc(video.path ?? "");
  },

  async startShare(folder, port) {
    return await startShareServer(folder, port);
  },

  async stopShare() {
    await stopShareServer();
  },

  async getShareStatus() {
    return await getShareStatusIpc();
  },

  async getPasswordStatus() {
    return await getPasswordStatusIpc();
  },
};
