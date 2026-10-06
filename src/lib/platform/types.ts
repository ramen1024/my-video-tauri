/**
 * 运行环境抽象层
 *
 * 桌面端与网页端**共用同一份前端代码**，差异全部收敛在这里：
 * - 桌面端（Tauri webview）：通过 IPC 与 Rust 通信，可弹目录选择框、调用系统播放器
 * - 网页端（局域网浏览器）：通过内嵌 HTTP 服务器的 JSON 接口通信
 *
 * 组件只依赖 [`Platform`] 接口与 `platform.kind`，不再关心自己跑在哪一侧。
 */

/** 应用内统一的视频条目 */
export interface VideoItem {
  /** 文件名（不含路径） */
  name: string;
  /** 相对于共享目录的路径，网页端播放与去重键 */
  relativePath: string;
  /** 文件大小（字节） */
  size: number;
  /** 修改时间，可能缺失 */
  modified: string | null;
  /** 文件扩展名（小写） */
  extension: string;
  /**
   * 本机绝对路径
   *
   * 仅桌面端存在：网页端接口（`GET /videos`）刻意不返回它，避免向局域网
   * 客户端泄露服务端的文件系统结构。
   */
  path?: string;
}

/** 运行环境标识 */
export type PlatformKind = "desktop" | "web";

/**
 * 当前运行环境提供的后端能力
 *
 * 所有方法都必须是"当前环境能实现"的：网页端不支持的能力用
 * `canPickFolder` / `canShare` 之类的只读标志显式声明，由 UI 决定是否渲染，
 * 而不是抛错后再补救。
 */
export interface Platform {
  /** 运行环境 */
  readonly kind: PlatformKind;
  /** 是否支持弹出本机目录选择框 */
  readonly canPickFolder: boolean;
  /** 是否支持局域网共享控制（启动/停止/状态/密码） */
  readonly canShare: boolean;
  /**
   * 列表自动刷新间隔（毫秒），`null` 表示不轮询
   *
   * 网页端可能被其他人通过 `/refresh` 改动列表，因此需要轮询；
   * 桌面端自己掌握扫描时机，轮询只会徒增开销。
   */
  readonly listPollIntervalMs: number | null;

  /** 选择本机文件夹；不支持或用户取消时返回 `null` */
  pickFolder(): Promise<string | null>;

  /** 重新扫描共享目录并返回最新列表 */
  rescan(folder: string): Promise<VideoItem[]>;

  /**
   * 读取当前视频列表（不触发扫描）
   *
   * 约定：**列表未变化时实现应返回同一个数组引用**，调用方据此跳过状态更新，
   * 避免轮询导致整表重渲染。桌面端只在初始化时调用，不做该优化。
   */
  loadVideos(): Promise<VideoItem[]>;

  /** 取消正在进行的扫描（网页端为无操作） */
  cancelScan(): Promise<void>;

  /** 该视频能否在应用内播放器里直接播放 */
  canPlayInline(video: VideoItem): boolean;

  /** 视频资源 URL（桌面端 asset 协议 / 网页端 HTTP 端点） */
  videoSrc(video: VideoItem): string;

  /** 用系统默认播放器打开（桌面端专用） */
  openWithSystemPlayer(video: VideoItem): Promise<void>;
}
