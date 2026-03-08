<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc } from "@tauri-apps/api/core";
  // @ts-ignore
  import qrcode from "qrcode-generator";

  let videos = $state([]);
  let currentFolder = $state("");
  let isScanning = $state(false);
  let errorMsg = $state("");
  let sortField = $state("name");
  let sortDirection = $state("asc");
  let currentVideo = $state(null);
  let isSharing = $state(false);
  let shareInfo = $state(null);
  let isStartingShare = $state(false);
  let qrCodeDataUrl = $state("");
  let selectedIp = $state("");
  let searchTerm = $state("");

  const supportedExtensions = ["mp4", "webm", "ogg", "mp4", "m4v"];
  const defaultSharePort = 6008;

  function generateQRCode(ip, port) {
    try {
      const url = `http://${ip}:${port}`;
      console.log("Generating QR code for:", url);
      const qr = qrcode(0, 'M');
      qr.addData(url);
      qr.make();
      qrCodeDataUrl = qr.createDataURL(4, 0);
      console.log("QR code generated:", qrCodeDataUrl ? "success" : "failed");
    } catch (e) {
      console.error("QR code generation failed:", e);
    }
  }

  function selectIp(ip) {
    selectedIp = ip;
    if (shareInfo) {
      generateQRCode(ip, shareInfo.port);
    }
  }

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
  
  let filteredVideos = $derived(
    searchTerm.trim() 
      ? sortedVideos.filter(v => v.name.toLowerCase().includes(searchTerm.toLowerCase()))
      : sortedVideos
  );

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

  async function startShare() {
    if (!currentFolder) {
      errorMsg = "请先选择文件夹";
      return;
    }
    
    isStartingShare = true;
    errorMsg = "";
    
    try {
      const result = await invoke("start_share_server", { 
        folderPath: currentFolder,
        port: defaultSharePort
      });
      shareInfo = result;
      isSharing = true;
      if (result.ips && result.ips.length > 0) {
        selectedIp = result.ips[result.ips.length - 1];
        generateQRCode(selectedIp, result.port);
      }
    } catch (e) {
      errorMsg = "开启共享失败: " + e;
    } finally {
      isStartingShare = false;
    }
  }

  async function stopShare() {
    try {
      await invoke("stop_share_server");
      isSharing = false;
      shareInfo = null;
    } catch (e) {
      errorMsg = "停止共享失败: " + e;
    }
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
        {#if isSharing}
          <button class="btn btn-danger" onclick={stopShare}>
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
              ><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect></svg
            >
            停止共享
          </button>
        {:else}
          <button class="btn btn-share" onclick={startShare} disabled={isStartingShare}>
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
              ><circle cx="18" cy="5" r="3"></circle><circle cx="6" cy="12" r="3"
              ></circle><circle cx="18" cy="19" r="3"></circle><line
                x1="8.59"
                y1="13.51"
                x2="15.42"
                y2="17.49"></line><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"
              ></line></svg
            >
            {isStartingShare ? "开启中..." : "局域网共享"}
          </button>
        {/if}
      {/if}
    </div>
  </header>

  {#if currentFolder}
    <div class="folder-path">
      <span class="path-label">当前文件夹:</span>
      <span class="path-value">{currentFolder}</span>
    </div>
  {/if}

  {#if isSharing && shareInfo}
    <div class="share-info">
      <span class="share-label">🎉 局域网共享已开启</span>
      <div class="share-content">
        {#if qrCodeDataUrl}
          <img src={qrCodeDataUrl} alt="二维码" class="qr-code" />
        {/if}
        <div class="share-address">
          <span>手机/平板扫描二维码或选择IP访问:</span>
          <div class="ip-list">
            {#each shareInfo.ips as ip}
              <button 
                class="ip-btn" 
                class:selected={ip === selectedIp}
                onclick={() => selectIp(ip)}
              >
                {ip}
              </button>
            {/each}
          </div>
          <a href="http://{selectedIp}:{shareInfo.port}" target="_blank" class="selected-link">
            http://{selectedIp}:{shareInfo.port}
          </a>
        </div>
      </div>
      <div class="firewall-hint">
        ⚠️ 如果其他设备无法访问，请在 Windows 防火墙中添加入站规则允许端口 {shareInfo.port}
      </div>
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
      <div class="video-toolbar">
        <div class="video-count">共找到 {videos.length} 个视频文件</div>
        <div class="search-box">
          <input 
            type="text" 
            placeholder="搜索视频..." 
            bind:value={searchTerm}
          />
        </div>
      </div>
      {#if filteredVideos.length === 0 && searchTerm}
        <div class="no-results">
          <p>没有找到匹配 "{searchTerm}" 的视频</p>
        </div>
      {:else}
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
              {#each filteredVideos as video}
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
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: #1a1a1a;
    overflow: hidden;
    min-height: 100vh;
  }

  @media (prefers-color-scheme: dark) {
    :global(body) {
      background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
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
    background: rgba(0, 0, 0, 0.95);
    backdrop-filter: blur(10px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .player-container {
    width: 90%;
    max-width: 1200px;
    background: #1a1a1a;
    border-radius: 16px;
    overflow: hidden;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
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
    padding: 16px 20px;
    background: rgba(255, 255, 255, 0.95);
    backdrop-filter: blur(10px);
    border-radius: 12px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
    margin-bottom: 16px;
  }

  @media (prefers-color-scheme: dark) {
    .header {
      background: rgba(45, 45, 45, 0.95);
      box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
    }
  }

  .title {
    font-size: 22px;
    font-weight: 700;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  @media (prefers-color-scheme: dark) {
    .title {
      background: linear-gradient(135deg, #a78bfa 0%, #818cf8 100%);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    border: none;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.3s ease;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-primary {
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 15px rgba(102, 126, 234, 0.4);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.9);
    color: #333;
  }

  @media (prefers-color-scheme: dark) {
    .btn-secondary {
      background: rgba(60, 60, 60, 0.9);
      color: #ffffff;
    }
  }

  .btn-secondary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 15px rgba(0, 0, 0, 0.15);
  }

  .btn-share {
    background: linear-gradient(135deg, #10b981 0%, #059669 100%);
    color: white;
  }

  .btn-share:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 15px rgba(16, 185, 129, 0.4);
  }

  .btn-share:disabled {
    background: #4b5563;
  }

  .btn-danger {
    background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
    color: white;
  }

  .btn-danger:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 15px rgba(239, 68, 68, 0.4);
  }

  .folder-path {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(255, 255, 255, 0.95);
    backdrop-filter: blur(10px);
    border-radius: 10px;
    margin-bottom: 12px;
    font-size: 13px;
    box-shadow: 0 4px 15px rgba(0, 0, 0, 0.1);
  }

  @media (prefers-color-scheme: dark) {
    .folder-path {
      background: rgba(45, 45, 45, 0.95);
    }
  }

  .share-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px 20px;
    background: rgba(255, 255, 255, 0.95);
    backdrop-filter: blur(10px);
    border-radius: 12px;
    margin-bottom: 12px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
  }

  @media (prefers-color-scheme: dark) {
    .share-info {
      background: rgba(45, 45, 45, 0.95);
    }
  }

  .share-label {
    font-weight: 600;
    color: #107c10;
    font-size: 14px;
  }

  .firewall-hint {
    font-size: 12px;
    color: #666;
    padding: 8px 12px;
    background: rgba(255, 193, 7, 0.15);
    border-radius: 4px;
    border-left: 3px solid #ffc107;
  }

  @media (prefers-color-scheme: dark) {
    .firewall-hint {
      color: #aaa;
      background: rgba(255, 193, 7, 0.1);
    }
  }

  @media (prefers-color-scheme: dark) {
    .share-label {
      color: #90caf9;
    }
  }

  .share-content {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    margin-top: 12px;
  }

  .qr-code {
    width: 120px;
    height: 120px;
    border-radius: 8px;
    background: white;
    padding: 8px;
  }

  .share-address {
    font-size: 13px;
    color: #0078d4;
    word-break: break-all;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .share-address a {
    color: #0078d4;
    text-decoration: none;
    font-size: 15px;
    font-weight: 500;
  }

  .share-address a:hover {
    text-decoration: underline;
  }

  .ip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 8px 0;
  }

  .ip-btn {
    padding: 6px 12px;
    border: 1px solid #ddd;
    border-radius: 6px;
    background: #fff;
    color: #333;
    font-size: 13px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .ip-btn:hover {
    border-color: #0078d4;
    color: #0078d4;
  }

  .ip-btn.selected {
    background: #0078d4;
    border-color: #0078d4;
    color: #fff;
  }

  .selected-link {
    font-size: 15px;
    font-weight: 500;
  }

  @media (prefers-color-scheme: dark) {
    .ip-btn {
      background: #2d2d2d;
      border-color: #444;
      color: #ccc;
    }
    .ip-btn:hover {
      border-color: #60a5fa;
      color: #60a5fa;
    }
    .ip-btn.selected {
      background: #0078d4;
      border-color: #0078d4;
      color: #fff;
    }
  }

  @media (prefers-color-scheme: dark) {
    .share-address {
      color: #60a5fa;
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
    background: rgba(255, 255, 255, 0.95);
    backdrop-filter: blur(10px);
    border-radius: 12px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  @media (prefers-color-scheme: dark) {
    .content {
      background: rgba(45, 45, 45, 0.95);
      box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
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

  .video-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid #e1e1e1;
    flex-shrink: 0;
    gap: 16px;
  }

  .video-count {
    font-size: 13px;
    color: #666;
  }

  .search-box {
    flex: 0 0 auto;
    max-width: 250px;
  }

  .search-box input {
    width: 100%;
    padding: 8px 12px;
    border: 1px solid #ddd;
    border-radius: 6px;
    font-size: 13px;
    transition: border-color 0.2s, box-shadow 0.2s;
  }

  .search-box input:focus {
    outline: none;
    border-color: #0078d4;
    box-shadow: 0 0 0 2px rgba(0, 120, 212, 0.2);
  }

  .no-results {
    padding: 40px 20px;
    text-align: center;
    color: #666;
  }

  @media (prefers-color-scheme: dark) {
    .video-toolbar {
      border-bottom-color: #3d3d3d;
    }
    .video-count {
      color: #999;
    }
    .search-box input {
      background: #2d2d2d;
      border-color: #444;
      color: #fff;
    }
    .search-box input:focus {
      border-color: #60a5fa;
      box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.2);
    }
    .no-results {
      color: #888;
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
