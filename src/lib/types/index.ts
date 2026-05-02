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
  videos: VideoFile[];
}

export interface PasswordStatus {
  enabled: boolean;
  has_password: boolean;
  password: string | null;
}

export type SortField = "name" | "size" | "modified";
export type SortDirection = "asc" | "desc";
