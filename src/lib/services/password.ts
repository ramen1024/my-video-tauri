/**
 * 密码服务模块
 *
 * 封装密码保护相关的 Tauri IPC 调用。
 */

import { invoke } from "@tauri-apps/api/core";
import type { PasswordStatus } from "$lib/types";

/** 获取密码保护状态 */
export async function getPasswordStatus(): Promise<PasswordStatus> {
  return await invoke("get_password_status");
}

/** 启用或禁用密码保护 */
export async function setPasswordEnabled(enabled: boolean): Promise<void> {
  await invoke("set_password_enabled", { enabled });
}

/** 设置密码（4 位数字） */
export async function setPassword(password: string): Promise<void> {
  await invoke("set_password", { password });
}

/** 生成 4 位随机数字密码 */
export async function generateRandomPassword(): Promise<string> {
  return await invoke("generate_random_password");
}

/** 重置密码（清除密码并禁用保护） */
export async function resetPassword(): Promise<void> {
  await invoke("reset_password");
}
