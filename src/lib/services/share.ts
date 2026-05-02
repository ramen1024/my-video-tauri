import { invoke } from "@tauri-apps/api/core";
import type { ShareServerInfo } from "$lib/types";

export async function startShareServer(
  folderPath: string,
  port: number,
): Promise<ShareServerInfo> {
  return await invoke("start_share_server", { folderPath, port });
}

export async function stopShareServer(): Promise<void> {
  await invoke("stop_share_server");
}
