<!--
  主页面
  应用的唯一页面：桌面端（Tauri）与网页端（局域网浏览器）共用同一份实现，
  环境差异由 $lib/platform 提供的后端与 platform.kind 决定。
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { platform, type VideoItem } from "$lib/platform";
  import type { ShareServerInfo, PasswordStatus } from "$lib/types";
  import { parseAppError } from "$lib/types";
  import { startShareServer, stopShareServer, getShareStatus } from "$lib/services/share";
  import { getPasswordStatus } from "$lib/services/password";
  import { DEFAULT_SHARE_PORT } from "$lib/config";
  import Header from "$lib/components/Header.svelte";
  import ErrorMessage from "$lib/components/ErrorMessage.svelte";
  import VideoPlayer from "$lib/components/VideoPlayer.svelte";
  import VideoTable from "$lib/components/VideoTable.svelte";
  import SharePanel from "$lib/components/SharePanel.svelte";
  import PasswordPanel from "$lib/components/PasswordPanel.svelte";
  import "$lib/styles/theme.css";
  import "$lib/styles/buttons.css";

  const isDesktop = platform.kind === "desktop";

  let videos = $state<VideoItem[]>([]);
  let currentFolder = $state("");
  let isScanning = $state(false);
  let errorMsg = $state("");
  let currentVideo = $state<VideoItem | null>(null);
  let isSharing = $state(false);
  let shareInfo = $state<ShareServerInfo | null>(null);
  let isStartingShare = $state(false);
  let isStoppingShare = $state(false);
  let passwordStatus = $state<PasswordStatus>({ enabled: false, has_password: false });
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function selectFolder() {
    try {
      const selected = await platform.pickFolder();
      if (!selected) return;
      // 换文件夹时先清空列表，避免扫描失败后残留上一个文件夹的内容；
      // 刷新同一个文件夹时保留旧列表，扫描完成后整体覆盖，避免界面闪空
      if (selected !== currentFolder) videos = [];
      currentFolder = selected;
      currentVideo = null;
      await doScan();
    } catch (e) {
      errorMsg = "选择文件夹失败: " + parseAppError(e);
    }
  }

  async function doScan() {
    if (platform.canPickFolder && !currentFolder) return;
    isScanning = true;
    errorMsg = "";
    try {
      videos = await platform.rescan(currentFolder);
    } catch (e) {
      const msg = parseAppError(e);
      errorMsg = msg.includes("扫描已取消") ? "扫描已取消" : "扫描失败: " + msg;
    } finally {
      isScanning = false;
    }
  }

  function handleCancelScan() {
    platform.cancelScan().catch((e) => {
      console.error("取消扫描失败:", e);
    });
  }

  async function playVideo(video: VideoItem) {
    if (platform.canPlayInline(video)) {
      errorMsg = "";
      currentVideo = video;
      return;
    }
    try {
      await platform.openWithSystemPlayer(video);
    } catch (e) {
      errorMsg = "无法播放该视频文件: " + parseAppError(e);
    }
  }

  async function startShare() {
    if (!currentFolder) {
      errorMsg = "请先选择文件夹";
      return;
    }
    isStartingShare = true;
    errorMsg = "";
    try {
      const result = await startShareServer(currentFolder, DEFAULT_SHARE_PORT);
      shareInfo = result;
      isSharing = true;
    } catch (e) {
      errorMsg = "开启共享失败: " + parseAppError(e);
    } finally {
      isStartingShare = false;
    }
  }

  async function stopShare() {
    isStoppingShare = true;
    try {
      await stopShareServer();
      isSharing = false;
      shareInfo = null;
    } catch (e) {
      errorMsg = "停止共享失败: " + parseAppError(e);
    } finally {
      isStoppingShare = false;
    }
  }

  /**
   * 拉取最新列表
   *
   * 后端约定"列表未变化时返回同一个数组引用"，据此跳过状态更新，
   * 轮询时不会造成整表重渲染。
   */
  async function refreshList() {
    const next = await platform.loadVideos();
    if (next !== videos) videos = next;
  }

  /**
   * 恢复后端已有的状态（仅桌面端）
   *
   * webview 重载（开发期 HMR、崩溃后 reload、菜单 reload）会清空前端状态，
   * 但后端的服务器可能仍在运行。不恢复的话界面会显示"局域网共享"未开启，
   * 点击又只会得到"服务器已在运行"，用户除了重启应用没办法停掉共享。
   */
  async function restoreBackendState() {
    try {
      const status = await getShareStatus();
      if (status.folder_path) {
        currentFolder = status.folder_path;
        videos = await platform.loadVideos();
      }
      if (status.running) {
        shareInfo = { ips: status.ips, port: status.port };
        isSharing = true;
      }
    } catch (e) {
      errorMsg = "恢复共享状态失败: " + parseAppError(e);
    }
  }

  onMount(async () => {
    if (isDesktop) {
      await restoreBackendState();
      try {
        passwordStatus = await getPasswordStatus();
      } catch (e) {
        console.error("获取密码状态失败:", e);
      }
    } else {
      // 网页端：初始加载 + 按平台约定的间隔轮询（列表可能被其他人刷新）
      try {
        await refreshList();
      } catch (e) {
        errorMsg = parseAppError(e);
      }
    }
  });

  $effect(() => {
    const interval = platform.listPollIntervalMs;
    if (interval === null) return;
    pollTimer = setInterval(() => {
      refreshList().catch((e) => {
        // 轮询失败不打断使用：仅记录，避免每 30 秒弹一次错误
        console.error("刷新列表失败:", e);
      });
    }, interval);
    return () => {
      if (pollTimer) clearInterval(pollTimer);
      pollTimer = null;
    };
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });
</script>

<main class="app">
  {#if currentVideo}
    <VideoPlayer
      video={currentVideo}
      videoSrc={platform.videoSrc(currentVideo)}
      onClose={() => { currentVideo = null; }}
      onError={(msg) => { errorMsg = msg; }}
    />
  {/if}

  <Header
    {isScanning}
    {isSharing}
    {currentFolder}
    {isStartingShare}
    {isStoppingShare}
    canPickFolder={platform.canPickFolder}
    canShare={platform.canShare}
    onSelectFolder={selectFolder}
    onScan={doScan}
    onStartShare={startShare}
    onStopShare={stopShare}
  />

  {#if isDesktop && currentFolder}
    <div class="folder-path">
      <span class="path-label">当前文件夹</span>
      <span class="path-value">{currentFolder}</span>
    </div>
  {/if}

  {#if isSharing && shareInfo}
    <SharePanel {shareInfo} />
    <PasswordPanel
      {passwordStatus}
      onStatusChange={(s) => { passwordStatus = s; }}
      onError={(msg) => { errorMsg = msg; }}
    />
  {/if}

  <ErrorMessage message={errorMsg} onDismiss={() => { errorMsg = ""; }} />

  <div class="content">
    {#if isScanning}
      <div class="loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p>正在扫描视频文件...</p>
        <button class="btn btn-danger" onclick={handleCancelScan}>取消扫描</button>
      </div>
    {:else if videos.length === 0}
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        <p>{platform.canPickFolder ? "请选择文件夹以扫描视频文件" : "共享文件夹中暂无可播放的视频"}</p>
      </div>
    {:else}
      <VideoTable {videos} onPlay={playVideo} canPlayInline={(v) => platform.canPlayInline(v)} />
    {/if}
  </div>
</main>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 0 20px 20px;
    background: var(--bg);
  }

  .folder-path {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 6px 0 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 12px;
  }

  .path-label {
    color: var(--text-tertiary);
    font-weight: 500;
    flex-shrink: 0;
  }

  .path-value {
    color: var(--text-secondary);
    word-break: break-all;
    font-family: var(--font-mono);
  }

  .content {
    flex: 1;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-secondary);
    gap: 16px;
  }

  .loading p {
    font-size: 13px;
  }

  .spinner {
    width: 36px;
    height: 36px;
    border: 2px solid var(--surface-raised);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-faint);
    padding: 32px;
    gap: 12px;
  }

  .empty-state svg {
    opacity: 0.5;
  }

  .empty-state p {
    font-size: 13px;
  }
</style>
