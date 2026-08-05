<!--
  主页面
  应用的唯一页面，编排所有子组件，管理全局状态。
  布局以工作区为核心，使用最小化的容器装饰。
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { VideoFile, ShareServerInfo, PasswordStatus } from "$lib/types";
  import { parseAppError } from "$lib/types";
  import { isSupportedFormat } from "$lib/utils/format";
  import { scanVideos, playVideo as playVideoFile, cancelScan } from "$lib/services/video";
  import { startShareServer, stopShareServer } from "$lib/services/share";
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

  let videos = $state<VideoFile[]>([]);
  let currentFolder = $state("");
  let isScanning = $state(false);
  let errorMsg = $state("");
  let currentVideo = $state<VideoFile | null>(null);
  let isSharing = $state(false);
  let shareInfo = $state<ShareServerInfo | null>(null);
  let isStartingShare = $state(false);
  let passwordStatus = $state<PasswordStatus>({ enabled: false, has_password: false });

  async function selectFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "选择视频文件夹" });
      if (selected) {
        currentFolder = selected;
        currentVideo = null;
        await doScan();
      }
    } catch (e) {
      errorMsg = "选择文件夹失败: " + parseAppError(e);
    }
  }

  async function doScan(useCache: boolean = true) {
    if (!currentFolder) return;
    isScanning = true;
    errorMsg = "";
    // 保留旧列表，扫描完成后直接覆盖，避免界面闪空
    try {
      videos = await scanVideos(currentFolder, useCache);
    } catch (e) {
      const msg = parseAppError(e);
      if (msg.includes("扫描已取消")) {
        errorMsg = "扫描已取消";
      } else {
        errorMsg = "扫描失败: " + msg;
      }
    } finally {
      isScanning = false;
    }
  }

  async function refreshScan() {
    await doScan(false);
  }

  function handleCancelScan() {
    cancelScan().catch(() => {});
  }

  function playVideo(video: VideoFile) {
    if (isSupportedFormat(video.extension)) {
      currentVideo = video;
    } else {
      playVideoFile(video.path).catch(e => {
        errorMsg = "无法播放该视频文件: " + parseAppError(e);
      });
    }
  }

  async function startShare() {
    if (!currentFolder) { errorMsg = "请先选择文件夹"; return; }
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
    try {
      await stopShareServer();
      isSharing = false;
      shareInfo = null;
    } catch (e) {
      errorMsg = "停止共享失败: " + parseAppError(e);
    }
  }

  onMount(async () => {
    try {
      passwordStatus = await getPasswordStatus();
    } catch (e) {
      console.error("获取密码状态失败:", e);
    }
  });
</script>

<main class="app">
  {#if currentVideo}
    <VideoPlayer video={currentVideo} onClose={() => { currentVideo = null; }} />
  {/if}

  <Header
    {isScanning}
    {isSharing}
    {currentFolder}
    {isStartingShare}
    onSelectFolder={selectFolder}
    onScan={refreshScan}
    onStartShare={startShare}
    onStopShare={stopShare}
  />

  {#if currentFolder}
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
      <div class="loading">
        <div class="spinner"></div>
        <p>正在扫描视频文件...</p>
        <button class="btn btn-danger" onclick={handleCancelScan}>取消扫描</button>
      </div>
    {:else if videos.length === 0}
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        <p>请选择文件夹以扫描视频文件</p>
      </div>
    {:else}
      <VideoTable {videos} onPlay={playVideo} />
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
