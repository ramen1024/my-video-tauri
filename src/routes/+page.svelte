<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";

  let videos = $state([]);
  let currentFolder = $state("");
  let isScanning = $state(false);
  let errorMsg = $state("");

  function formatFileSize(bytes) {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  async function selectFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择视频文件夹",
      });

      if (selected) {
        currentFolder = selected;
        await scanVideos();
      }
    } catch (e) {
      errorMsg = "选择文件夹失败: " + e;
    }
  }

  async function scanVideos() {
    if (!currentFolder) return;

    isScanning = true;
    errorMsg = "";
    videos = [];

    try {
      videos = await invoke("scan_videos", { folderPath: currentFolder });
    } catch (e) {
      errorMsg = "扫描失败: " + e;
    } finally {
      isScanning = false;
    }
  }

  async function playVideo(videoPath) {
    try {
      await invoke("play_video", { filePath: videoPath });
    } catch (e) {
      errorMsg = "无法播放该视频文件: " + e;
    }
  }
</script>

<main class="app">
  <header class="header">
    <h1 class="title">视频播放器</h1>
    <div class="actions">
      <button class="btn btn-primary" onclick={selectFolder}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg
        >
        选择文件夹
      </button>
      {#if currentFolder}
        <button class="btn btn-secondary" onclick={scanVideos} disabled={isScanning}>
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class:spinning={isScanning}
            ><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"></path><path
              d="M21 3v5h-5"></path></svg
          >
          刷新
        </button>
      {/if}
    </div>
  </header>

  {#if currentFolder}
    <div class="folder-path">
      <span class="path-label">当前文件夹:</span>
      <span class="path-value">{currentFolder}</span>
    </div>
  {/if}

  {#if errorMsg}
    <div class="error-message">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        ><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"
        ></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg
      >
      {errorMsg}
    </div>
  {/if}

  <div class="content">
    {#if isScanning}
      <div class="loading">
        <div class="spinner"></div>
        <p>正在扫描视频文件...</p>
      </div>
    {:else if videos.length === 0}
      <div class="empty-state">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="64"
          height="64"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
          ></path></svg
        >
        <p>请选择文件夹以扫描视频文件</p>
      </div>
    {:else}
      <div class="video-count">共找到 {videos.length} 个视频文件</div>
      <div class="video-list">
        {#each videos as video, index}
          <button class="video-item" onclick={() => playVideo(video.path)}>
            <div class="video-icon">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="24"
                height="24"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><polygon points="5 3 19 12 5 21 5 3"></polygon></svg
              >
            </div>
            <div class="video-info">
              <div class="video-name">{video.name}</div>
              <div class="video-meta">
                <span class="video-size">{formatFileSize(video.size)}</span>
                <span class="video-ext">.{video.extension}</span>
              </div>
              <div class="video-path">{video.path}</div>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</main>

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(body) {
    font-family: "Segoe UI Variable", "Segoe UI", sans-serif;
    background-color: #f3f3f3;
    color: #1a1a1a;
    overflow: hidden;
  }

  @media (prefers-color-scheme: dark) {
    :global(body) {
      background-color: #202020;
      color: #ffffff;
    }
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 16px;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #ffffff;
    border-radius: 8px;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
    margin-bottom: 16px;
  }

  @media (prefers-color-scheme: dark) {
    .header {
      background: #2d2d2d;
      box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
    }
  }

  .title {
    font-size: 20px;
    font-weight: 600;
    color: #0078d4;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-primary {
    background: #0078d4;
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    background: #106ebe;
  }

  .btn-secondary {
    background: #e1e1e1;
    color: #1a1a1a;
  }

  @media (prefers-color-scheme: dark) {
    .btn-secondary {
      background: #3d3d3d;
      color: #ffffff;
    }
  }

  .btn-secondary:hover:not(:disabled) {
    background: #c8c8c8;
  }

  @media (prefers-color-scheme: dark) {
    .btn-secondary:hover:not(:disabled) {
      background: #4d4d4d;
    }
  }

  .folder-path {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: #ffffff;
    border-radius: 6px;
    margin-bottom: 12px;
    font-size: 13px;
  }

  @media (prefers-color-scheme: dark) {
    .folder-path {
      background: #2d2d2d;
    }
  }

  .path-label {
    color: #666;
    font-weight: 500;
  }

  @media (prefers-color-scheme: dark) {
    .path-label {
      color: #999;
    }
  }

  .path-value {
    color: #0078d4;
    word-break: break-all;
  }

  .error-message {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: #fef0f0;
    color: #c42b1c;
    border-radius: 6px;
    margin-bottom: 12px;
    font-size: 13px;
  }

  @media (prefers-color-scheme: dark) {
    .error-message {
      background: #3d2020;
      color: #ff9999;
    }
  }

  .content {
    flex: 1;
    background: #ffffff;
    border-radius: 8px;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
    overflow: hidden;
  }

  @media (prefers-color-scheme: dark) {
    .content {
      background: #2d2d2d;
      box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
    }
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid #e1e1e1;
    border-top-color: #0078d4;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #999;
    padding: 40px;
  }

  .empty-state svg {
    margin-bottom: 16px;
    opacity: 0.5;
  }

  .video-count {
    padding: 12px 16px;
    font-size: 13px;
    color: #666;
    border-bottom: 1px solid #e1e1e1;
  }

  @media (prefers-color-scheme: dark) {
    .video-count {
      color: #999;
      border-bottom-color: #3d3d3d;
    }
  }

  .video-list {
    height: calc(100% - 45px);
    overflow-y: auto;
    padding: 8px;
  }

  .video-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px;
    background: transparent;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    transition: background-color 0.15s;
    margin-bottom: 4px;
  }

  .video-item:hover {
    background: #f5f5f5;
  }

  @media (prefers-color-scheme: dark) {
    .video-item:hover {
      background: #3d3d3d;
    }
  }

  .video-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    background: #e1e1e1;
    border-radius: 8px;
    flex-shrink: 0;
  }

  @media (prefers-color-scheme: dark) {
    .video-icon {
      background: #3d3d3d;
    }
  }

  .video-icon svg {
    color: #0078d4;
  }

  .video-info {
    flex: 1;
    min-width: 0;
  }

  .video-name {
    font-size: 14px;
    font-weight: 500;
    color: #1a1a1a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (prefers-color-scheme: dark) {
    .video-name {
      color: #ffffff;
    }
  }

  .video-meta {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    font-size: 12px;
    color: #666;
  }

  @media (prefers-color-scheme: dark) {
    .video-meta {
      color: #999;
    }
  }

  .video-ext {
    background: #e1e1e1;
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 500;
  }

  @media (prefers-color-scheme: dark) {
    .video-ext {
      background: #3d3d3d;
    }
  }

  .video-path {
    font-size: 11px;
    color: #999;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 4px;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }
</style>
