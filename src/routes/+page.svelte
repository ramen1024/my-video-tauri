<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let videos = $state([]);
  let currentFolder = $state("");
  let isScanning = $state(false);
  let errorMsg = $state("");
  let sortField = $state("name");
  let sortDirection = $state("asc");
  let currentVideo = $state(null);

  const supportedExtensions = ["mp4", "webm", "ogg", "mp4", "m4v"];

  function isSupportedFormat(ext) {
    return supportedExtensions.includes(ext.toLowerCase());
  }

  function formatFileSize(bytes) {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  function sortVideos(list, field, direction) {
    return [...list].sort((a, b) => {
      let valA = a[field];
      let valB = b[field];

      if (field === "size") {
        valA = Number(valA);
        valB = Number(valB);
      } else if (field === "modified") {
        valA = valA || "";
        valB = valB || "";
      } else {
        valA = String(valA).toLowerCase();
        valB = String(valB).toLowerCase();
      }

      if (valA < valB) return direction === "asc" ? -1 : 1;
      if (valA > valB) return direction === "asc" ? 1 : -1;
      return 0;
    });
  }

  let sortedVideos = $derived(sortVideos(videos, sortField, sortDirection));

  function toggleSort(field) {
    if (sortField === field) {
      sortDirection = sortDirection === "asc" ? "desc" : "asc";
    } else {
      sortField = field;
      sortDirection = "asc";
    }
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
        currentVideo = null;
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

  function playVideo(video) {
    if (isSupportedFormat(video.extension)) {
      currentVideo = video;
    } else {
      invoke("play_video", { filePath: video.path }).catch((e) => {
        errorMsg = "无法播放该视频文件: " + e;
      });
    }
  }

  function closePlayer() {
    currentVideo = null;
  }

  function getVideoSrc(videoPath) {
    const normalizedPath = videoPath.replace(/\\/g, "/");
    return convertFileSrc(normalizedPath);
  }
</script>

<main class="app">
  {#if currentVideo}
    <div class="player-overlay">
      <div class="player-container">
        <div class="player-header">
          <span class="player-title">{currentVideo.name}</span>
          <button class="close-btn" onclick={closePlayer}>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              ><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6"
                x2="18"
                y2="18"></line></svg
            >
          </button>
        </div>
        <video
          src={getVideoSrc(currentVideo.path)}
          controls
          autoplay
          class="video-player"
        >
          您的浏览器不支持视频播放
        </video>
      </div>
    </div>
  {/if}

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
        <p class="hint">提示：WallpaperEngine 视频 workshop 路径一般为 E:\Steam\steamapps\workshop\content\431960</p>
      </div>
    {:else}
      <div class="video-count">共找到 {videos.length} 个视频文件</div>
      <div class="table-container">
        <table class="video-table">
          <thead>
            <tr>
              <th class="col-play"></th>
              <th class="col-name sortable" onclick={() => toggleSort("name")}>
                文件名
                {#if sortField === "name"}
                  <span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>
                {/if}
              </th>
              <th class="col-size sortable" onclick={() => toggleSort("size")}>
                大小
                {#if sortField === "size"}
                  <span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>
                {/if}
              </th>
              <th class="col-date sortable" onclick={() => toggleSort("modified")}>
                日期
                {#if sortField === "modified"}
                  <span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>
                {/if}
              </th>
              <th class="col-type">播放</th>
            </tr>
          </thead>
          <tbody>
            {#each sortedVideos as video}
              <tr>
                <td class="col-play">
                  <div class="play-icon">
                    <svg
                      xmlns="http://www.w3.org/2000/svg"
                      width="16"
                      height="16"
                      viewBox="0 0 24 24"
                      fill="currentColor"
                      ><polygon points="5 3 19 12 5 21 5 3"></polygon></svg
                    >
                  </div>
                </td>
                <td class="col-name">
                  <span class="video-name">{video.name}</span>
                  <span class="video-ext">.{video.extension}</span>
                </td>
                <td class="col-size">{formatFileSize(video.size)}</td>
                <td class="col-date">{video.modified || "-"}</td>
                <td class="col-type">
                  {#if isSupportedFormat(video.extension)}
                    <button class="play-btn" onclick={() => playVideo(video)}>播放</button>
                  {:else}
                    <button class="system-btn" onclick={() => playVideo(video)}>系统</button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
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
    position: relative;
  }

  .player-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.9);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .player-container {
    width: 90%;
    max-width: 1200px;
    background: #1a1a1a;
    border-radius: 12px;
    overflow: hidden;
  }

  @media (prefers-color-scheme: dark) {
    .player-container {
      background: #1a1a1a;
    }
  }

  .player-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #2d2d2d;
  }

  .player-title {
    color: #fff;
    font-size: 14px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close-btn {
    background: transparent;
    border: none;
    color: #fff;
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .video-player {
    width: 100%;
    display: block;
    max-height: 80vh;
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
    display: flex;
    flex-direction: column;
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

  .empty-state .hint {
    margin-top: 16px;
    font-size: 12px;
    color: #0078d4;
    opacity: 0.8;
  }

  @media (prefers-color-scheme: dark) {
    .empty-state .hint {
      color: #60a5fa;
    }
  }

  .video-count {
    padding: 12px 16px;
    font-size: 13px;
    color: #666;
    border-bottom: 1px solid #e1e1e1;
    flex-shrink: 0;
  }

  @media (prefers-color-scheme: dark) {
    .video-count {
      color: #999;
      border-bottom-color: #3d3d3d;
    }
  }

  .table-container {
    flex: 1;
    overflow: auto;
  }

  .video-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .video-table th {
    position: sticky;
    top: 0;
    background: #f5f5f5;
    padding: 12px 16px;
    text-align: left;
    font-weight: 600;
    color: #666;
    border-bottom: 1px solid #e1e1e1;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }

  .video-table th.sortable::after {
    content: "⇅";
    margin-left: 6px;
    opacity: 0.3;
    font-size: 12px;
  }

  .video-table th.sortable:hover::after {
    opacity: 0.7;
  }

  .video-table th:hover {
    background: #ebebeb;
  }

  @media (prefers-color-scheme: dark) {
    .video-table th {
      background: #3d3d3d;
      color: #999;
      border-bottom-color: #4d4d4d;
    }
  }

  @media (prefers-color-scheme: dark) {
    .video-table th:hover {
      background: #4d4d4d;
    }
  }

  .sort-icon {
    margin-left: 4px;
    font-size: 10px;
    color: #0078d4;
  }

  .video-table td {
    padding: 10px 16px;
    border-bottom: 1px solid #f0f0f0;
  }

  @media (prefers-color-scheme: dark) {
    .video-table td {
      border-bottom-color: #3d3d3d;
    }
  }

  .video-table tr {
    cursor: pointer;
    transition: background-color 0.15s;
  }

  .video-table tbody tr:hover {
    background: #f5f5f5;
  }

  @media (prefers-color-scheme: dark) {
    .video-table tbody tr:hover {
      background: #3d3d3d;
    }
  }

  .col-play {
    width: 50px;
    text-align: center;
  }

  .col-name {
    min-width: 200px;
  }

  .col-size {
    width: 100px;
    text-align: right;
  }

  .col-date {
    width: 180px;
  }

  .col-type {
    width: 80px;
    text-align: center;
  }

  .play-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: #0078d4;
    border-radius: 50%;
    color: white;
  }

  .video-name {
    font-weight: 500;
  }

  .video-ext {
    margin-left: 6px;
    color: #666;
    font-size: 12px;
  }

  @media (prefers-color-scheme: dark) {
    .video-ext {
      color: #999;
    }
  }

  .play-btn,
  .system-btn {
    padding: 4px 12px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.2s;
  }

  .play-btn {
    background: #0078d4;
    color: white;
  }

  .play-btn:hover {
    background: #106ebe;
  }

  .system-btn {
    background: #e1e1e1;
    color: #1a1a1a;
  }

  @media (prefers-color-scheme: dark) {
    .system-btn {
      background: #3d3d3d;
      color: #ffffff;
    }
  }

  .system-btn:hover {
    background: #c8c8c8;
  }

  @media (prefers-color-scheme: dark) {
    .system-btn:hover {
      background: #4d4d4d;
    }
  }

  .spinning {
    animation: spin 1s linear infinite;
  }
</style>
