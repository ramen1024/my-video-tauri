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
    <h1 class="title">视频扫描器</h1>
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
  /* ============================================
     赛博朋克主题样式
     配色方案:
     - 霓虹黄: #F0E100 (主强调色)
     - 青色: #00F0FF (次强调色)
     - 洋红: #FF0066 (危险/警告)
     - 深黑: #0A0A0F (主背景)
     - 暗灰: #12121A (次背景)
     - 边框灰: #2A2A3A
     ============================================ */

  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(body) {
    font-family: "Rajdhani", "Segoe UI", "Microsoft YaHei", sans-serif;
    background: #0A0A0F;
    color: #E0E0E0;
    overflow: hidden;
    min-height: 100vh;
  }

  /* 扫描线背景效果 */
  :global(body::before) {
    content: "";
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: repeating-linear-gradient(
      0deg,
      transparent,
      transparent 2px,
      rgba(0, 240, 255, 0.03) 2px,
      rgba(0, 240, 255, 0.03) 4px
    );
    pointer-events: none;
    z-index: 9999;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 20px;
    position: relative;
    background: #0A0A0F;
  }

  /* ============================================
     播放器弹窗 - 赛博朋克风格
     ============================================ */
  .player-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(10, 10, 15, 0.95);
    backdrop-filter: blur(10px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .player-container {
    width: 90%;
    max-width: 1200px;
    background: #12121A;
    border: 1px solid #F0E100;
    box-shadow:
      0 0 20px rgba(240, 225, 0, 0.3),
      0 0 40px rgba(240, 225, 0, 0.1),
      inset 0 0 20px rgba(240, 225, 0, 0.05);
    clip-path: polygon(
      0 10px,
      10px 0,
      calc(100% - 10px) 0,
      100% 10px,
      100% calc(100% - 10px),
      calc(100% - 10px) 100%,
      10px 100%,
      0 calc(100% - 10px)
    );
  }

  .player-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    background: linear-gradient(90deg, #12121A 0%, #1A1A25 50%, #12121A 100%);
    border-bottom: 1px solid #F0E100;
  }

  .player-title {
    color: #F0E100;
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-transform: uppercase;
    letter-spacing: 1px;
    text-shadow: 0 0 10px rgba(240, 225, 0, 0.5);
  }

  .close-btn {
    background: transparent;
    border: 1px solid #FF0066;
    color: #FF0066;
    cursor: pointer;
    padding: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s ease;
    clip-path: polygon(20% 0%, 100% 0%, 100% 80%, 80% 100%, 0% 100%, 0% 20%);
  }

  .close-btn:hover {
    background: #FF0066;
    color: #0A0A0F;
    box-shadow: 0 0 15px rgba(255, 0, 102, 0.6);
  }

  .video-player {
    width: 100%;
    display: block;
    max-height: 80vh;
    background: #000;
  }

  /* ============================================
     头部区域 - 赛博朋克风格
     ============================================ */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 24px;
    background: linear-gradient(135deg, #12121A 0%, #1A1A25 100%);
    border: 1px solid #F0E100;
    margin-bottom: 20px;
    position: relative;
    clip-path: polygon(
      0 0,
      calc(100% - 20px) 0,
      100% 20px,
      100% 100%,
      20px 100%,
      0 calc(100% - 20px)
    );
  }

  .header::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: linear-gradient(90deg, transparent, #F0E100, transparent);
    animation: scanline 3s linear infinite;
  }

  @keyframes scanline {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  .title {
    font-size: 28px;
    font-weight: 700;
    color: #F0E100;
    text-transform: uppercase;
    letter-spacing: 3px;
    text-shadow:
      0 0 10px rgba(240, 225, 0, 0.5),
      0 0 20px rgba(240, 225, 0, 0.3),
      0 0 30px rgba(240, 225, 0, 0.1);
  }

  .actions {
    display: flex;
    gap: 12px;
  }

  /* ============================================
     按钮 - 赛博朋克风格（斜切角设计）
     ============================================ */
  .btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px;
    border: 1px solid;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.3s ease;
    background: transparent;
    text-transform: uppercase;
    letter-spacing: 1px;
    position: relative;
    overflow: hidden;
    clip-path: polygon(10% 0%, 100% 0%, 100% 70%, 90% 100%, 0% 100%, 0% 30%);
  }

  .btn::before {
    content: "";
    position: absolute;
    top: 0;
    left: -100%;
    width: 100%;
    height: 100%;
    background: linear-gradient(90deg, transparent, rgba(255,255,255,0.2), transparent);
    transition: left 0.5s ease;
  }

  .btn:hover::before {
    left: 100%;
  }

  .btn:hover:not(:disabled) {
    transform: translateY(-2px);
  }

  .btn:active:not(:disabled) {
    transform: translateY(0);
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    filter: grayscale(100%);
  }

  /* 主按钮 - 霓虹黄 */
  .btn-primary {
    border-color: #F0E100;
    color: #F0E100;
    background: rgba(240, 225, 0, 0.1);
  }

  .btn-primary:hover:not(:disabled) {
    background: #F0E100;
    color: #0A0A0F;
    box-shadow:
      0 0 10px rgba(240, 225, 0, 0.5),
      0 0 20px rgba(240, 225, 0, 0.3),
      0 0 30px rgba(240, 225, 0, 0.1);
  }

  /* 次按钮 - 青色 */
  .btn-secondary {
    border-color: #00F0FF;
    color: #00F0FF;
    background: rgba(0, 240, 255, 0.1);
  }

  .btn-secondary:hover:not(:disabled) {
    background: #00F0FF;
    color: #0A0A0F;
    box-shadow:
      0 0 10px rgba(0, 240, 255, 0.5),
      0 0 20px rgba(0, 240, 255, 0.3);
  }

  /* 分享按钮 - 橙色 */
  .btn-share {
    border-color: #FF6600;
    color: #FF6600;
    background: rgba(255, 102, 0, 0.1);
  }

  .btn-share:hover:not(:disabled) {
    background: #FF6600;
    color: #0A0A0F;
    box-shadow:
      0 0 10px rgba(255, 102, 0, 0.5),
      0 0 20px rgba(255, 102, 0, 0.3);
  }

  /* 危险按钮 - 红色 */
  .btn-danger {
    border-color: #FF0066;
    color: #FF0066;
    background: rgba(255, 0, 102, 0.1);
  }

  .btn-danger:hover:not(:disabled) {
    background: #FF0066;
    color: #0A0A0F;
    box-shadow:
      0 0 10px rgba(255, 0, 102, 0.5),
      0 0 20px rgba(255, 0, 102, 0.3);
  }

  /* ============================================
     文件夹路径 - 赛博朋克风格
     ============================================ */
  .folder-path {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    background: #12121A;
    border: 1px solid #2A2A3A;
    border-left: 3px solid #00F0FF;
    margin-bottom: 16px;
    font-size: 13px;
    position: relative;
  }

  .folder-path::before {
    content: ">>>";
    position: absolute;
    right: 20px;
    color: #00F0FF;
    opacity: 0.5;
    font-size: 10px;
    letter-spacing: 2px;
  }

  .path-label {
    color: #00F0FF;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .path-value {
    color: #E0E0E0;
    word-break: break-all;
    font-weight: 500;
    font-family: "Courier New", monospace;
  }

  /* ============================================
     共享信息面板 - 赛博朋克风格
     ============================================ */
  .share-info {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 24px;
    background: linear-gradient(135deg, #12121A 0%, #1A1A25 100%);
    border: 1px solid #FF6600;
    margin-bottom: 16px;
    position: relative;
    clip-path: polygon(
      0 0,
      calc(100% - 15px) 0,
      100% 15px,
      100% 100%,
      15px 100%,
      0 calc(100% - 15px)
    );
  }

  .share-info::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 2px;
    background: linear-gradient(90deg, #FF6600, transparent);
  }

  .share-label {
    font-weight: 700;
    color: #FF6600;
    font-size: 14px;
    text-transform: uppercase;
    letter-spacing: 2px;
    text-shadow: 0 0 10px rgba(255, 102, 0, 0.5);
  }

  .firewall-hint {
    font-size: 11px;
    color: #888;
    padding: 12px 16px;
    background: rgba(255, 0, 102, 0.1);
    border-left: 2px solid #FF0066;
    font-family: "Courier New", monospace;
  }

  .share-content {
    display: flex;
    align-items: flex-start;
    gap: 20px;
    margin-top: 12px;
  }

  .qr-code {
    width: 120px;
    height: 120px;
    background: #0A0A0F;
    padding: 8px;
    border: 1px solid #00F0FF;
    filter: drop-shadow(0 0 10px rgba(0, 240, 255, 0.3));
  }

  .share-address {
    font-size: 13px;
    color: #00F0FF;
    word-break: break-all;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .share-address a {
    color: #F0E100;
    text-decoration: none;
    font-size: 15px;
    font-weight: 600;
    font-family: "Courier New", monospace;
    text-shadow: 0 0 10px rgba(240, 225, 0, 0.3);
  }

  .share-address a:hover {
    text-shadow: 0 0 20px rgba(240, 225, 0, 0.6);
  }

  .ip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 8px 0;
  }

  .ip-btn {
    padding: 8px 16px;
    border: 1px solid #2A2A3A;
    background: transparent;
    color: #888;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    font-family: "Courier New", monospace;
    clip-path: polygon(10% 0%, 100% 0%, 100% 70%, 90% 100%, 0% 100%, 0% 30%);
  }

  .ip-btn:hover {
    border-color: #00F0FF;
    color: #00F0FF;
    box-shadow: 0 0 10px rgba(0, 240, 255, 0.3);
  }

  .ip-btn.selected {
    border-color: #F0E100;
    color: #F0E100;
    background: rgba(240, 225, 0, 0.1);
    box-shadow: 0 0 15px rgba(240, 225, 0, 0.3);
  }

  .selected-link {
    font-size: 15px;
    font-weight: 600;
  }

  /* ============================================
     错误消息 - 赛博朋克风格
     ============================================ */
  .error-message {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
    background: rgba(255, 0, 102, 0.1);
    color: #FF0066;
    border: 1px solid #FF0066;
    border-left: 4px solid #FF0066;
    margin-bottom: 16px;
    font-size: 13px;
    font-weight: 500;
    animation: errorPulse 2s ease-in-out infinite;
  }

  @keyframes errorPulse {
    0%, 100% { box-shadow: 0 0 5px rgba(255, 0, 102, 0.3); }
    50% { box-shadow: 0 0 20px rgba(255, 0, 102, 0.6); }
  }

  /* ============================================
     内容区域 - 赛博朋克风格
     ============================================ */
  .content {
    flex: 1;
    background: #12121A;
    border: 1px solid #2A2A3A;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .content::before {
    content: "";
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, #00F0FF, transparent);
  }

  /* ============================================
     加载状态 - 赛博朋克风格
     ============================================ */
  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #00F0FF;
    gap: 16px;
  }

  .loading p {
    text-transform: uppercase;
    letter-spacing: 2px;
    animation: textFlicker 2s infinite;
  }

  @keyframes textFlicker {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .spinner {
    width: 50px;
    height: 50px;
    border: 2px solid #2A2A3A;
    border-top-color: #F0E100;
    border-right-color: #00F0FF;
    animation: spin 1s linear infinite;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ============================================
     空状态 - 赛博朋克风格
     ============================================ */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #888;
    padding: 40px;
  }

  .empty-state svg {
    margin-bottom: 16px;
    opacity: 0.3;
    color: #00F0FF;
  }

  .empty-state p {
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .empty-state .hint {
    margin-top: 16px;
    font-size: 11px;
    color: #F0E100;
    opacity: 0.6;
    font-family: "Courier New", monospace;
  }

  /* ============================================
     工具栏 - 赛博朋克风格
     ============================================ */
  .video-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid #2A2A3A;
    flex-shrink: 0;
    gap: 16px;
    background: linear-gradient(90deg, #12121A 0%, #1A1A25 100%);
  }

  .video-count {
    font-size: 13px;
    color: #00F0FF;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .search-box {
    flex: 0 0 auto;
    max-width: 250px;
    position: relative;
  }

  .search-box::before {
    content: ">>>";
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: #F0E100;
    font-size: 10px;
    letter-spacing: 2px;
  }

  .search-box input {
    width: 100%;
    padding: 10px 14px 10px 40px;
    border: 1px solid #2A2A3A;
    font-size: 13px;
    background: #0A0A0F;
    color: #E0E0E0;
    transition: all 0.2s ease;
    font-family: "Courier New", monospace;
  }

  .search-box input:focus {
    outline: none;
    border-color: #00F0FF;
    box-shadow: 0 0 10px rgba(0, 240, 255, 0.3);
  }

  .search-box input::placeholder {
    color: #555;
  }

  .no-results {
    padding: 40px 20px;
    text-align: center;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  /* ============================================
     视频表格 - 赛博朋克风格
     ============================================ */
  .table-container {
    flex: 1;
    overflow: auto;
  }

  .video-table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-size: 13px;
  }

  .video-table th {
    position: sticky;
    top: 0;
    background: #0A0A0F;
    padding: 14px 20px;
    text-align: left;
    font-weight: 700;
    color: #F0E100;
    border-bottom: 2px solid #F0E100;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    text-transform: uppercase;
    letter-spacing: 1px;
    font-size: 11px;
  }

  .video-table th.sortable::after {
    content: "⇅";
    margin-left: 6px;
    opacity: 0.4;
    font-size: 12px;
    color: #00F0FF;
  }

  .video-table th.sortable:hover::after {
    opacity: 0.8;
  }

  .video-table th:hover {
    color: #00F0FF;
    text-shadow: 0 0 10px rgba(0, 240, 255, 0.5);
  }

  .sort-icon {
    margin-left: 4px;
    font-size: 10px;
    color: #F0E100;
    text-shadow: 0 0 5px rgba(240, 225, 0, 0.5);
  }

  .video-table td {
    padding: 12px 20px;
    border-bottom: 1px solid #2A2A3A;
  }

  .video-table tr {
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .video-table tbody tr:hover {
    background: rgba(0, 240, 255, 0.05);
    border-left: 2px solid #00F0FF;
  }

  .video-table tbody tr:hover td:first-child {
    padding-left: 18px;
  }

  .col-play {
    width: 60px;
    text-align: center;
  }

  .col-name {
    min-width: 200px;
  }

  .col-size {
    width: 100px;
    text-align: right;
    font-family: "Courier New", monospace;
    color: #00F0FF;
  }

  .col-date {
    width: 180px;
    font-family: "Courier New", monospace;
    color: #888;
  }

  .col-type {
    width: 80px;
    text-align: center;
  }

  .play-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: transparent;
    border: 1px solid #F0E100;
    color: #F0E100;
    transition: all 0.2s ease;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
  }

  .play-icon:hover {
    background: #F0E100;
    color: #0A0A0F;
    box-shadow: 0 0 15px rgba(240, 225, 0, 0.5);
  }

  .video-name {
    font-weight: 600;
    color: #E0E0E0;
  }

  .video-ext {
    margin-left: 6px;
    color: #F0E100;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
  }

  .play-btn,
  .system-btn {
    padding: 6px 14px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid;
    transition: all 0.2s ease;
    background: transparent;
    text-transform: uppercase;
    letter-spacing: 1px;
    clip-path: polygon(10% 0%, 100% 0%, 100% 70%, 90% 100%, 0% 100%, 0% 30%);
  }

  .play-btn {
    border-color: #00F0FF;
    color: #00F0FF;
  }

  .play-btn:hover {
    background: #00F0FF;
    color: #0A0A0F;
    box-shadow: 0 0 10px rgba(0, 240, 255, 0.5);
  }

  .system-btn {
    border-color: #888;
    color: #888;
  }

  .system-btn:hover {
    border-color: #FF0066;
    color: #FF0066;
    box-shadow: 0 0 10px rgba(255, 0, 102, 0.3);
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  /* 滚动条样式 */
  .table-container::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }

  .table-container::-webkit-scrollbar-track {
    background: #0A0A0F;
  }

  .table-container::-webkit-scrollbar-thumb {
    background: #2A2A3A;
    border: 1px solid #00F0FF;
  }

  .table-container::-webkit-scrollbar-thumb:hover {
    background: #00F0FF;
  }
</style>
