/**
 * 共享服务模块
 *
 * 封装局域网共享服务器的 Tauri IPC 调用。
 */

import { invoke } from "@tauri-apps/api/core";
import type { ShareServerInfo } from "$lib/types";

/** 启动局域网共享服务器 */
export async function startShareServer(
  folderPath: string,
  port: number,
): Promise<ShareServerInfo> {
  return await invoke("start_share_server", { folderPath, port });
}

/** 停止局域网共享服务器 */
export async function stopShareServer(): Promise<void> {
  await invoke("stop_share_server");
}
