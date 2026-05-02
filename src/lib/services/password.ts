import { invoke } from "@tauri-apps/api/core";
import type { PasswordStatus } from "$lib/types";

export async function getPasswordStatus(): Promise<PasswordStatus> {
  return await invoke("get_password_status");
}

export async function setPasswordEnabled(enabled: boolean): Promise<void> {
  await invoke("set_password_enabled", { enabled });
}

export async function setPassword(password: string): Promise<void> {
  await invoke("set_password", { password });
}

export async function generateRandomPassword(): Promise<string> {
  return await invoke("generate_random_password");
}

export async function resetPassword(): Promise<void> {
  await invoke("reset_password");
}
