export interface VideoFile {
  name: string;
  path: string;
  relative_path: string;
  size: number;
  modified: string | null;
  extension: string;
}

export interface ShareServerInfo {
  ips: string[];
  port: number;
}

export interface PasswordStatus {
  enabled: boolean;
  has_password: boolean;
}

export type SortField = "name" | "size" | "modified";
export type SortDirection = "asc" | "desc";

export interface AppError {
  type: "InvalidPath" | "ScanCancelled" | "ServerAlreadyRunning" | "ServerNotRunning" | "IoError" | "PasswordError" | "Other";
  message: string;
}

export function parseAppError(e: unknown): string {
  if (typeof e === "object" && e !== null) {
    const err = e as Record<string, unknown>;
    if (err.type && typeof err.type === "string" && err.message && typeof err.message === "string") {
      return String(err.message);
    }
  }
  return String(e);
}
