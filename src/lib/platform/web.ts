/**
 * 网页端（局域网浏览器）后端实现
 *
 * 通过内嵌 HTTP 服务器的 JSON 接口取数据。这里的逻辑原先内联在第二套网页模板
 * （`html_template.html` 的 `<script>`）里——那正是"两套前端"问题的来源；
 * 现在它只是同一份前端代码的一个后端实现。
 *
 * 接口契约见 `docs/api.md`。
 */

import type { Platform, VideoItem } from "./types";

/** 与 Rust `VideoSummary` 对应：网页端接口刻意不返回本机绝对路径 */
interface VideoSummary {
  name: string;
  relative_path: string;
  size: number;
  modified: string | null;
  extension: string;
}

interface RefreshStatus {
  success?: boolean;
  pending?: boolean;
  message?: string;
}

const VIDEO_LIST_PATH = "/videos";
const REFRESH_PATH = "/refresh";
const REFRESH_STATUS_PATH = "/refresh-status";

/** 大文件夹全量扫描可能耗时较长，轮询上限放宽到 2 分钟 */
const REFRESH_TIMEOUT_MS = 120_000;
const REFRESH_POLL_INTERVAL_MS = 800;
/** 会话有效时长的保守估计无关紧要：服务端才是权威，这里只做请求超时保护 */
const REQUEST_HINT = "无法连接到服务器，请检查网络";

/** 上一次成功拉取的列表与指纹，用于"列表未变化时返回同一引用" */
let cachedList: VideoItem[] = [];
let cachedEtag: string | null = null;

function toVideoItem(summary: VideoSummary): VideoItem {
  return {
    name: summary.name,
    relativePath: summary.relative_path,
    size: summary.size,
    modified: summary.modified,
    extension: summary.extension,
    // 网页端不持有本机绝对路径，播放统一走 /video/<relativePath>
    path: undefined,
  };
}

/** 会话失效：把浏览器送回登录页 */
function handleSessionExpired(): never {
  // 后端对未认证请求 302 到 /login，fetch 会跟随并拿到 HTML。
  // 重新加载页面即可回到登录页；已经停在 /login 上时不再触发，避免循环刷新。
  if (typeof window !== "undefined" && window.location.pathname !== "/login") {
    window.location.reload();
  }
  throw new Error("登录已过期，请重新登录");
}

/** 判断响应是否其实是登录页（302 被跟随，或直接 401） */
function isAuthFailure(response: Response): boolean {
  return response.redirected || response.status === 401;
}

/** 读取 JSON 响应体，非 JSON 时给出基于状态码的提示 */
async function readJson(response: Response): Promise<Record<string, unknown>> {
  try {
    return (await response.json()) as Record<string, unknown>;
  } catch {
    throw new Error(`服务器返回了非 JSON 响应（HTTP ${response.status}）`);
  }
}

/** 发起取 JSON 的请求，统一处理会话失效与错误码 */
async function requestJson(url: string): Promise<{ response: Response; data: Record<string, unknown> }> {
  let response: Response;
  try {
    response = await fetch(url, { cache: "no-store" });
  } catch {
    throw new Error(REQUEST_HINT);
  }

  if (isAuthFailure(response)) handleSessionExpired();

  if (!response.ok) {
    let message = `请求失败（HTTP ${response.status}）`;
    try {
      const data = (await response.json()) as RefreshStatus;
      if (typeof data.message === "string" && data.message) message = data.message;
    } catch {
      // 错误体不是 JSON：保留状态码提示
    }
    throw new Error(message);
  }

  return { response, data: await readJson(response) };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 拉取列表；列表未变化时返回同一个数组引用，供调用方跳过重渲染 */
async function loadVideosFromServer(): Promise<VideoItem[]> {
  let response: Response;
  try {
    const headers: Record<string, string> = { "Cache-Control": "no-cache" };
    if (cachedEtag) headers["If-None-Match"] = cachedEtag;
    response = await fetch(VIDEO_LIST_PATH, { cache: "no-store", headers });
  } catch {
    throw new Error(REQUEST_HINT);
  }

  if (isAuthFailure(response)) handleSessionExpired();

  // 服务端返回 304：内容未变化，直接复用同一份列表
  if (response.status === 304) return cachedList;

  if (!response.ok) {
    let message = `获取视频列表失败（HTTP ${response.status}）`;
    try {
      const data = (await response.json()) as RefreshStatus;
      if (typeof data.message === "string" && data.message) message = data.message;
    } catch {
      // 忽略：保留状态码提示
    }
    throw new Error(message);
  }

  const etag = response.headers.get("ETag");
  if (etag && etag === cachedEtag && cachedList.length > 0) return cachedList;

  const summaries = (await response.json()) as VideoSummary[];
  cachedEtag = etag;
  cachedList = summaries.map(toVideoItem);
  return cachedList;
}

/** 触发重新扫描并等待结果 */
async function rescanOnServer(): Promise<VideoItem[]> {
  const { data } = await requestJson(REFRESH_PATH);
  if (!data.success) {
    throw new Error(typeof data.message === "string" ? data.message : "刷新失败");
  }

  const deadline = Date.now() + REFRESH_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const { data: status } = await requestJson(REFRESH_STATUS_PATH);
    // pending 表示后端还没有本次刷新的结果，继续轮询；
    // 不依赖 message 文案，避免后端改提示语导致这里误判
    if (!status.pending) {
      const result = status as RefreshStatus;
      if (!result.success) {
        throw new Error(result.message || "刷新失败，请重试");
      }
      // 列表已变，丢弃指纹缓存，强制重新拉取
      cachedEtag = null;
      return await loadVideosFromServer();
    }
    await sleep(REFRESH_POLL_INTERVAL_MS);
  }

  throw new Error("刷新超时，请稍后重试");
}

export const web: Platform = {
  kind: "web",
  canPickFolder: false,
  canShare: false,
  listPollIntervalMs: 30_000,

  async pickFolder() {
    // 网页端无法访问客户端文件系统；由 canPickFolder 保证 UI 不会提供该入口
    return null;
  },

  async rescan() {
    return await rescanOnServer();
  },

  async loadVideos() {
    return await loadVideosFromServer();
  },

  async cancelScan() {
    // 网页端无法中断服务端的扫描任务，等待其自然结束
  },

  canPlayInline() {
    // 一律交给浏览器内置播放器尝试；不支持的编码由播放器 error 事件给出提示
    return true;
  },

  videoSrc(video) {
    // 服务端 /video/* 会拒绝非视频扩展名，并对路径做穿越防护
    return `${"/video/"}${encodeURIComponent(video.relativePath)}`;
  },

  async openWithSystemPlayer() {
    // 网页端没有"系统播放器"概念；canPlayInline 恒为 true，此分支不会被触及
    throw new Error("网页端不支持调用系统播放器");
  },
};
