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

  let passwordEnabled = $state(false);
  let hasPassword = $state(false);
  let currentPassword = $state("");
  let pwdPanelExpanded = $state(false);
  let pwdInput = $state("");
  let pwdErrorMsg = $state("");
  let pwdSubmitting = $state(false);
  let pwdCopied = $state(false);
  let popIndex = $state(-1);
  let maskedIndices = $state(new Set());
  let pwdSuccess = $state(false);

  const supportedExtensions = ["mp4", "webm", "ogg", "m4v"];
  const defaultSharePort = 6008;

  function generateQRCode(ip, port) {
    try {
      const url = `http://${ip}:${port}`;
      console.log("Generating QR code for:", url);
      const qr = qrcode(0, 'M');
      qr.addData(url);
      qr.make();
      qrCodeDataUrl = qr.createDataURL(4, 0);
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
      await invoke("scan_videos", { folderPath: currentFolder });
      videos = await invoke("get_shared_videos");
    } catch (e) {
      if (e === "扫描已取消") {
        errorMsg = "扫描已取消";
      } else {
        errorMsg = "扫描失败: " + e;
      }
    } finally {
      isScanning = false;
    }
  }

  function cancelScanning() {
    invoke("cancel_scan");
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

  // ===== 密码功能（内嵌式面板） =====

  async function loadPasswordStatus() {
    try {
      const status = await invoke("get_password_status");
      passwordEnabled = status.enabled;
      hasPassword = status.has_password;
      currentPassword = status.password || "";
    } catch (e) {
      console.error("获取密码状态失败:", e);
    }
  }

  async function toggleProtection() {
    if (!hasPassword && !passwordEnabled) {
      pwdPanelExpanded = true;
      return;
    }
    try {
      await invoke("set_password_enabled", { enabled: !passwordEnabled });
      passwordEnabled = !passwordEnabled;
    } catch (e) {
      errorMsg = "切换密码保护失败: " + e;
    }
  }

  async function copyPassword() {
    try {
      await navigator.clipboard.writeText(currentPassword);
      pwdCopied = true;
      setTimeout(() => { pwdCopied = false; }, 1500);
    } catch (e) {
      errorMsg = "复制失败: " + e;
    }
  }

  async function randomGenerateAndApply() {
    try {
      const newPwd = await invoke("generate_random_password");
      await invoke("set_password", { password: newPwd });
      currentPassword = newPwd;
      hasPassword = true;
      if (!passwordEnabled) {
        await invoke("set_password_enabled", { enabled: true });
        passwordEnabled = true;
      }
      pwdInput = "";
      pwdErrorMsg = "";
    } catch (e) {
      pwdErrorMsg = "设置密码失败: " + e;
    }
  }

  function handleNumpadDigit(digit) {
    if (pwdSubmitting || pwdInput.length >= 4) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    const idx = pwdInput.length;
    popIndex = idx;
    pwdInput += digit;
    const newMasked = new Set(maskedIndices);
    maskedIndices = newMasked;
    setTimeout(() => {
      maskedIndices = new Set([...maskedIndices, idx]);
      popIndex = -1;
    }, 600);
    setTimeout(() => { popIndex = -1; }, 220);
    if (pwdInput.length === 4) {
      submitNewPassword();
    }
  }

  function handleNumpadBackspace() {
    if (pwdSubmitting) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    pwdInput = pwdInput.slice(0, -1);
    const newMasked = new Set();
    for (let i = 0; i < pwdInput.length; i++) newMasked.add(i);
    maskedIndices = newMasked;
  }

  function handleNumpadClear() {
    if (pwdSubmitting) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    pwdInput = "";
    maskedIndices = new Set();
  }

  async function submitNewPassword() {
    if (pwdInput.length !== 4) return;
    pwdSubmitting = true;
    try {
      await invoke("set_password", { password: pwdInput });
      currentPassword = pwdInput;
      hasPassword = true;
      if (!passwordEnabled) {
        await invoke("set_password_enabled", { enabled: true });
        passwordEnabled = true;
      }
      pwdSuccess = true;
      setTimeout(() => {
        pwdInput = "";
        pwdErrorMsg = "";
        pwdSuccess = false;
        maskedIndices = new Set();
        pwdPanelExpanded = false;
      }, 1200);
    } catch (e) {
      pwdErrorMsg = "设置失败: " + e;
      pwdInput = "";
      maskedIndices = new Set();
    } finally {
      pwdSubmitting = false;
    }
  }

  async function clearPassword() {
    try {
      await invoke("reset_password");
      passwordEnabled = false;
      hasPassword = false;
      currentPassword = "";
      pwdInput = "";
      pwdPanelExpanded = false;
      maskedIndices = new Set();
      popIndex = -1;
      pwdSuccess = false;
    } catch (e) {
      errorMsg = "清除密码失败: " + e;
    }
  }

  function togglePwdPanel() {
    pwdPanelExpanded = !pwdPanelExpanded;
    pwdInput = "";
    pwdErrorMsg = "";
    pwdSuccess = false;
    maskedIndices = new Set();
    popIndex = -1;
  }

  $effect(() => {
    loadPasswordStatus();
  });
</script>

<main class="app">
  {#if currentVideo}
    <div class="player-overlay">
      <div class="player-container">
        <div class="player-header">
          <span class="player-title">{currentVideo.name}</span>
          <button class="close-btn" onclick={closePlayer} aria-label="关闭播放器">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
          </button>
        </div>
        <video src={getVideoSrc(currentVideo.path)} controls autoplay class="video-player">您的浏览器不支持视频播放</video>
      </div>
    </div>
  {/if}

  <header class="header">
    <h1 class="title">视频扫描器</h1>
    <div class="actions">
      <button class="btn btn-primary" onclick={selectFolder}>
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        选择文件夹
      </button>
      {#if currentFolder}
        <button class="btn btn-secondary" onclick={scanVideos} disabled={isScanning}>
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class:spinning={isScanning}><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"></path><path d="M21 3v5h-5"></path></svg>
          刷新
        </button>
        {#if isSharing}
          <button class="btn btn-danger" onclick={stopShare}>
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect></svg>
            停止共享
          </button>
        {:else}
          <button class="btn btn-share" onclick={startShare} disabled={isStartingShare}>
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5" r="3"></circle><circle cx="6" cy="12" r="3"></circle><circle cx="18" cy="19" r="3"></circle><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"></line><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"></line></svg>
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
      <div class="share-info-header">
        <span class="share-label">🎉 局域网共享已开启</span>

        <!-- 密码保护开关 -->
        <div class="protection-toggle">
          <div class="protection-status" class:on={passwordEnabled} class:off={!passwordEnabled}>
            <span class="status-dot"></span>
            <span>{passwordEnabled ? "已开启" : "未开启"}</span>
          </div>
          <button class="toggle-btn" class:active={passwordEnabled} onclick={toggleProtection}>
            {passwordEnabled ? "关闭" : "开启"}
          </button>
        </div>
      </div>

      <!-- 密码区域：根据状态显示不同内容 -->
      <div class="pwd-section">
        {#if hasPassword && currentPassword}
          <!-- 已有密码：显示密码+操作按钮 -->
          <div class="pwd-show-area">
            <div class="pwd-display-row">
              <span class="pwd-hint-text">{passwordEnabled ? "访问密码" : "已保存密码"}:</span>
              <div class="pwd-digits">
                {#each currentPassword.split('') as digit}
                  <span class="pwd-digit">{digit}</span>
                {/each}
              </div>
            </div>
            <div class="pwd-actions-row">
              <button class="action-btn random-btn" onclick={randomGenerateAndApply}>
                🔄 换一个
              </button>
              <button class="action-btn copy-btn" onclick={copyPassword}>
                {pwdCopied ? "✅ 已复制" : "📋 复制"}
              </button>
              <button class="action-btn clear-btn" onclick={clearPassword}>🗑️ 清除</button>
            </div>
          </div>
        {:else if !hasPassword}
          <!-- 未设置密码：引导设置 -->
          <div class="pwd-setup-area">
            <div class="setup-hint">🔓 设置4位数字密码，局域网访问需输入此密码</div>
            <button class="setup-random-btn" onclick={randomGenerateAndApply}>
              🎲 随机生成并启动保护
            </button>
            <button class="setup-manual-btn" onclick={togglePwdPanel}>
              ✏️ 手动输入密码
            </button>
          </div>
        {/if}

        <!-- 手动输入面板（可折叠） -->
        {#if pwdPanelExpanded}
          <div class="pwd-input-panel">
            <div class="panel-label">输入 4 位数字密码</div>
            <div class="pin-display">
              {#each Array(4) as _, i}
                <div class="pin-box" class:has-digit={i < pwdInput.length} class:pop-in={popIndex === i && pwdInput.length > i}>
                  {#if i < pwdInput.length && !maskedIndices.has(i)}
                    {pwdInput[i]}
                  {:else if i < pwdInput.length}
                    <span class="pin-dot-inner"></span>
                  {/if}
                </div>
              {/each}
            </div>

            {#if pwdErrorMsg}
              <div class="pwd-error-banner">
                <span>⚠️</span>
                <span>{pwdErrorMsg}</span>
              </div>
            {:else if pwdSubmitting}
              <div class="pwd-loading-banner">
                <span class="ld-dot"></span>
                <span class="ld-dot"></span>
                <span class="ld-dot"></span>
                <span>设置中</span>
              </div>
            {:else if pwdSuccess}
              <div class="pwd-success-banner">
                <span>✓</span>
                <span>密码设置成功</span>
              </div>
            {/if}

            <div class="numpad-wrap">
              <div class="numpad-overlay" class:active={pwdSubmitting}>
                <div class="overlay-dots">
                  <span class="od-dot"></span>
                  <span class="od-dot"></span>
                  <span class="od-dot"></span>
                </div>
              </div>
              <div class="numpad" class:dimmed={pwdSubmitting}>
                <button class="num-key" onclick={() => handleNumpadDigit("1")}>1</button>
                <button class="num-key" onclick={() => handleNumpadDigit("2")}>2</button>
                <button class="num-key" onclick={() => handleNumpadDigit("3")}>3</button>
                <button class="num-key" onclick={() => handleNumpadDigit("4")}>4</button>
                <button class="num-key" onclick={() => handleNumpadDigit("5")}>5</button>
                <button class="num-key" onclick={() => handleNumpadDigit("6")}>6</button>
                <button class="num-key" onclick={() => handleNumpadDigit("7")}>7</button>
                <button class="num-key" onclick={() => handleNumpadDigit("8")}>8</button>
                <button class="num-key" onclick={() => handleNumpadDigit("9")}>9</button>
                <button class="num-key action-key" onclick={handleNumpadClear}>清除</button>
                <button class="num-key" onclick={() => handleNumpadDigit("0")}>0</button>
                <button class="num-key delete-key" onclick={handleNumpadBackspace}>⌫</button>
              </div>
            </div>
          </div>
        {/if}
      </div>

      <div class="share-content">
        {#if qrCodeDataUrl}
          <img src={qrCodeDataUrl} alt="二维码" class="qr-code" />
        {/if}
        <div class="share-address">
          <span>手机/平板扫描二维码或选择IP访问:</span>
          <div class="ip-list">
            {#each shareInfo.ips as ip}
              <button class="ip-btn" class:selected={ip === selectedIp} onclick={() => selectIp(ip)}>
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
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>
      {errorMsg}
    </div>
  {/if}

  <div class="content">
    {#if isScanning}
      <div class="loading">
        <div class="spinner"></div>
        <p>正在扫描视频文件...</p>
        <button class="btn btn-danger" onclick={cancelScanning}>取消扫描</button>
      </div>
    {:else if videos.length === 0}
      <div class="empty-state">
        <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        <p>请选择文件夹以扫描视频文件</p>
        <p class="hint">提示：WallpaperEngine 视频 workshop 路径一般为 E:\Steam\steamapps\workshop\content\431960</p>
      </div>
    {:else}
      <div class="video-toolbar">
        <div class="video-count">共找到 {videos.length} 个视频文件</div>
        <div class="search-box">
          <input type="text" placeholder="搜索视频..." bind:value={searchTerm} />
        </div>
      </div>
      {#if filteredVideos.length === 0 && searchTerm}
        <div class="no-results"><p>没有找到匹配 "{searchTerm}" 的视频</p></div>
      {:else}
        <div class="table-container">
          <table class="video-table">
            <thead>
              <tr>
                <th class="col-play"></th>
                <th class="col-name sortable" onclick={() => toggleSort("name")}>
                  文件名
                  {#if sortField === "name"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
                </th>
                <th class="col-size sortable" onclick={() => toggleSort("size")}>
                  大小
                  {#if sortField === "size"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
                </th>
                <th class="col-date sortable" onclick={() => toggleSort("modified")}>
                  日期
                  {#if sortField === "modified"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
                </th>
                <th class="col-type">播放</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredVideos as video}
              <tr>
                <td class="col-play">
                  <button class="play-icon" onclick={() => playVideo(video)} aria-label="播放视频">
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
                  </button>
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
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(body) { font-family: "Segoe UI", "Microsoft YaHei", -apple-system, BlinkMacSystemFont, sans-serif; background: #0f172a; color: #ffffff; overflow: hidden; min-height: 100vh; }

  .app { display: flex; flex-direction: column; height: 100vh; padding: 16px; position: relative; background: transparent; }

  /* ===== 播放器 ===== */
  .player-overlay { position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0,0,0,.6); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px); z-index: 1000; display: flex; align-items: center; justify-content: center; }
  .player-container { width: 90%; max-width: 1200px; background: rgba(255,255,255,.1); backdrop-filter: blur(20px); border: 1px solid rgba(255,255,255,.2); border-radius: 16px; box-shadow: 0 8px 32px rgba(0,0,0,.4); overflow: hidden; }
  .player-header { display: flex; justify-content: space-between; align-items: center; padding: 12px 20px; background: rgba(255,255,255,.05); border-bottom: 1px solid rgba(255,255,255,.1); }
  .player-title { font-size: 15px; font-weight: 500; color: #fff; }
  .close-btn { background: rgba(255,255,255,.1); border: 1px solid rgba(255,255,255,.2); color: #fff; cursor: pointer; padding: 8px; display: flex; align-items: center; justify-content: center; transition: all .2s ease; border-radius: 8px; }
  .close-btn:hover { background: rgba(239,68,68,.3); border-color: rgba(239,68,68,.5); }
  .video-player { width: 100%; display: block; max-height: 80vh; background: #000; }

  /* ===== 头部 ===== */
  .header { display: flex; justify-content: space-between; align-items: center; padding: 16px 20px; background: rgba(255,255,255,.06); backdrop-filter: blur(20px); border: 1px solid rgba(255,255,255,.1); border-radius: 16px; margin-bottom: 12px; box-shadow: 0 4px 20px rgba(0,0,0,.2); }
  .title { font-size: 22px; font-weight: 600; color: #fff; letter-spacing: .3px; }
  .actions { display: flex; gap: 8px; }

  /* ===== 按钮 ===== */
  .btn { display: flex; align-items: center; gap: 6px; padding: 10px 18px; border: 1px solid rgba(255,255,255,.15); font-size: 13px; font-weight: 500; cursor: pointer; transition: all .2s ease; background: rgba(255,255,255,.08); color: #fff; border-radius: 10px; }
  .btn:hover:not(:disabled) { background: rgba(255,255,255,.14); transform: translateY(-1px); }
  .btn:active:not(:disabled) { transform: translateY(0); }
  .btn:disabled { opacity: .4; cursor: not-allowed; }
  .btn-primary { background: rgba(59,130,246,.2); border-color: rgba(59,130,246,.4); color: #93c5fd; }
  .btn-primary:hover:not(:disabled) { background: rgba(59,130,246,.35); color: #bfdbfe; }
  .btn-secondary { background: rgba(139,92,246,.2); border-color: rgba(139,92,246,.4); color: #c4b5fd; }
  .btn-secondary:hover:not(:disabled) { background: rgba(139,92,246,.35); color: #ddd6fe; }
  .btn-share { background: rgba(6,182,212,.2); border-color: rgba(6,182,212,.4); color: #67e8f9; }
  .btn-share:hover:not(:disabled) { background: rgba(6,182,212,.35); color: #a5f3fc; }
  .btn-danger { background: rgba(239,68,68,.2); border-color: rgba(239,68,68,.4); color: #fca5a5; }
  .btn-danger:hover:not(:disabled) { background: rgba(239,68,68,.35); color: #fecaca; }

  /* ===== 文件夹路径 ===== */
  .folder-path { display: flex; align-items: center; gap: 8px; padding: 12px 16px; background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.08); border-radius: 12px; margin-bottom: 12px; font-size: 13px; }
  .path-label { color: rgba(255,255,255,.5); font-weight: 500; white-space: nowrap; }
  .path-value { color: rgba(255,255,255,.8); word-break: break-all; }

  /* ===== 共享信息 ===== */
  .share-info { display: flex; flex-direction: column; gap: 12px; padding: 16px; background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.08); border-radius: 16px; margin-bottom: 12px; }
  .share-info-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 10px; }
  .share-label { font-weight: 600; color: #22d3ee; font-size: 13px; }

  /* ===== 密码保护开关 ===== */
  .protection-toggle { display: flex; align-items: center; gap: 8px; }
  .protection-status { display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 20px; font-size: 12px; font-weight: 500; }
  .protection-status.on { background: rgba(34,197,94,.12); border: 1px solid rgba(34,197,94,.25); color: #4ade80; }
  .protection-status.off { background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.08); color: rgba(255,255,255,.4); }
  .status-dot { width: 6px; height: 6px; border-radius: 50%; display: inline-block; }
  .protection-status.on .status-dot { background: #4ade80; box-shadow: 0 0 6px rgba(74,222,128,.5); }
  .protection-status.off .status-dot { background: rgba(255,255,255,.25); }
  .toggle-btn { padding: 4px 12px; font-size: 12px; font-weight: 500; border-radius: 8px; cursor: pointer; transition: all .2s ease; border: 1px solid rgba(255,255,255,.12); background: rgba(255,255,255,.06); color: rgba(255,255,255,.7); }
  .toggle-btn.active { background: rgba(34,197,94,.15); border-color: rgba(34,197,94,.3); color: #4ade80; }
  .toggle-btn:hover { background: rgba(255,255,255,.1); }
  .toggle-btn.active:hover { background: rgba(34,197,94,.25); }

  /* ===== 密码区域 ===== */
  .pwd-section { display: flex; flex-direction: column; gap: 8px; padding: 12px; background: rgba(59,130,246,.04); border: 1px solid rgba(59,130,246,.1); border-radius: 12px; margin-top: 4px; }
  .pwd-show-area { display: flex; flex-direction: column; gap: 8px; }
  .pwd-display-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .pwd-hint-text { font-size: 12px; color: rgba(255,255,255,.5); white-space: nowrap; }
  .pwd-digits { display: flex; gap: 6px; }
  .pwd-digit { font-size: 22px; font-weight: 700; color: #60a5fa; width: 32px; height: 38px; display: flex; align-items: center; justify-content: center; background: rgba(96,165,250,.1); border: 1px solid rgba(96,165,250,.2); border-radius: 8px; }
  .pwd-actions-row { display: flex; gap: 6px; flex-wrap: wrap; }
  .action-btn { padding: 5px 12px; font-size: 12px; font-weight: 500; cursor: pointer; border-radius: 8px; border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.05); color: rgba(255,255,255,.65); transition: all .2s ease; }
  .action-btn:hover { background: rgba(255,255,255,.1); }
  .random-btn:hover { border-color: rgba(34,197,94,.3); color: #4ade80; background: rgba(34,197,94,.08); }
  .copy-btn:hover { border-color: rgba(96,165,250,.3); color: #60a5fa; background: rgba(96,165,250,.08); }
  .clear-btn:hover { border-color: rgba(239,68,68,.3); color: #fca5a5; background: rgba(239,68,68,.08); }

  /* 未设置密码的引导区 */
  .pwd-setup-area { display: flex; flex-direction: column; gap: 8px; align-items: center; }
  .setup-hint { font-size: 12px; color: rgba(255,255,255,.45); text-align: center; }
  .setup-random-btn { padding: 8px 16px; font-size: 12px; font-weight: 600; border-radius: 10px; cursor: pointer; border: 1px solid rgba(34,197,94,.25); background: rgba(34,197,94,.12); color: #4ade80; transition: all .2s ease; }
  .setup-random-btn:hover { background: rgba(34,197,94,.22); }
  .setup-manual-btn { padding: 6px 14px; font-size: 12px; font-weight: 500; border-radius: 8px; cursor: pointer; border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.04); color: rgba(255,255,255,.55); transition: all .2s ease; }
  .setup-manual-btn:hover { background: rgba(255,255,255,.08); color: rgba(255,255,255,.8); }

  /* 内嵌数字键盘面板 */
  .pwd-input-panel { display: flex; flex-direction: column; align-items: stretch; gap: 8px; padding: 12px 16px 10px; background: rgba(15,23,42,.5); border: 1px solid rgba(255,255,255,.06); border-radius: 12px; margin-top: 4px; animation: slideDown .2s cubic-bezier(.16,1,.3,1); }
  @keyframes slideDown { from { opacity: 0; transform: translateY(-4px) scale(.98); } to { opacity: 1; transform: translateY(0) scale(1); } }
  .panel-label { font-size: 12px; color: rgba(255,255,255,.45); text-align: center; }
  .pin-display { display: flex; justify-content: center; gap: 8px; }
  .pin-box { width: 34px; height: 42px; border-radius: 8px; background: rgba(255,255,255,.05); border: 1.5px solid rgba(255,255,255,.12); display: flex; align-items: center; justify-content: center; font-size: 20px; font-weight: 700; color: transparent; transition: all .2s ease; position: relative; overflow: hidden; }
  .pin-box.has-digit { color: #60a5fa; border-color: rgba(96,165,250,.3); background: rgba(96,165,250,.06); }
  .pin-box.pop-in { animation: popInBox .2s cubic-bezier(.34,1.56,.64,1); }
  @keyframes popInBox { 0% { transform: scale(.6); opacity: 0; } 100% { transform: scale(1); opacity: 1; } }
  .pin-dot-inner { width: 8px; height: 8px; border-radius: 50%; background: #60a5fa; box-shadow: 0 0 6px rgba(96,165,250,.4); display: block; }

  .pwd-error-banner { display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px; background: rgba(239,68,68,.1); border: 1px solid rgba(239,68,68,.2); border-radius: 8px; color: #fca5a5; font-size: 11px; font-weight: 500; animation: bannerIn .2s ease; }
  .pwd-loading-banner { display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px; color: rgba(255,255,255,.5); font-size: 11px; font-weight: 500; }
  .pwd-success-banner { display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px; background: rgba(34,197,94,.1); border: 1px solid rgba(34,197,94,.22); border-radius: 8px; color: #4ade80; font-size: 11px; font-weight: 600; animation: bannerIn .2s ease; }
  @keyframes bannerIn { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: translateY(0); } }
  .ld-dot, .od-dot { display: inline-block; width: 4px; height: 4px; border-radius: 50%; background: currentColor; animation: dotBounce 1.2s ease-in-out infinite; }
  .ld-dot:nth-child(2), .od-dot:nth-child(2) { animation-delay: .15s; }
  .ld-dot:nth-child(3), .od-dot:nth-child(3) { animation-delay: .3s; }
  @keyframes dotBounce { 0%,80%,100% { opacity: .3; transform: translateY(0); } 40% { opacity: 1; transform: translateY(-3px); } }

  /* 数字键盘 */
  .numpad-wrap { position: relative; }
  .numpad-overlay { position: absolute; inset: 0; background: rgba(15,23,42,.5); backdrop-filter: blur(2px); border-radius: 10px; display: flex; align-items: center; justify-content: center; z-index: 5; opacity: 0; pointer-events: none; transition: opacity .2s ease; }
  .numpad-overlay.active { opacity: 1; pointer-events: auto; }
  .overlay-dots { display: flex; gap: 5px; }
  .od-dot { background: rgba(255,255,255,.5); }
  .numpad { display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px; width: 100%; transition: opacity .2s ease; }
  .numpad.dimmed { opacity: .4; }
  .num-key { height: 42px; border: 1px solid rgba(255,255,255,.1); border-radius: 8px; background: rgba(255,255,255,.05); color: #fff; font-size: 17px; font-weight: 500; cursor: pointer; transition: all .15s ease; user-select: none; position: relative; overflow: hidden; }
  .num-key:hover { background: rgba(255,255,255,.1); border-color: rgba(96,165,250,.25); }
  .num-key:active { background: rgba(96,165,250,.18); transform: scale(.96); }
  .num-key.action-key { font-size: 12px; color: rgba(255,255,255,.45); }
  .num-key.delete-key { font-size: 15px; color: rgba(255,255,255,.6); }
  .num-key.delete-key:hover { background: rgba(239,68,68,.12); border-color: rgba(239,68,68,.25); color: #fca5a5; }

  /* ===== 防火墙提示 ===== */
  .firewall-hint { font-size: 12px; color: rgba(255,255,255,.5); padding: 10px 14px; background: rgba(239,68,68,.06); border: 1px solid rgba(239,68,68,.12); border-radius: 8px; }

  /* ===== 分享内容 ===== */
  .share-content { display: flex; align-items: flex-start; gap: 16px; margin-top: 8px; }
  .qr-code { width: 112px; height: 112px; background: rgba(255,255,255,.06); padding: 10px; border-radius: 10px; border: 1px solid rgba(255,255,255,.12); }
  .share-address { font-size: 13px; color: rgba(255,255,255,.7); word-break: break-all; display: flex; flex-direction: column; gap: 6px; }
  .share-address a { color: #60a5fa; text-decoration: none; font-size: 14px; font-weight: 600; transition: all .2s ease; }
  .share-address a:hover { color: #93c5fd; }
  .ip-list { display: flex; flex-wrap: wrap; gap: 8px; margin: 4px 0; }
  .ip-btn { padding: 6px 12px; border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.05); color: rgba(255,255,255,.6); font-size: 12px; font-weight: 500; cursor: pointer; transition: all .2s ease; border-radius: 8px; }
  .ip-btn:hover { border-color: rgba(96,165,250,.3); color: #60a5fa; background: rgba(255,255,255,.08); }
  .ip-btn.selected { border-color: #fbbf24; color: #fbbf24; background: rgba(251,191,36,.08); }
  .selected-link { font-size: 14px; font-weight: 600; }

  /* ===== 错误消息 ===== */
  .error-message { display: flex; align-items: center; gap: 8px; padding: 12px 16px; background: rgba(239,68,68,.1); color: #fca5a5; border: 1px solid rgba(239,68,68,.2); border-radius: 12px; margin-bottom: 12px; font-size: 13px; font-weight: 500; }

  /* ===== 内容区 ===== */
  .content { flex: 1; background: rgba(255,255,255,.03); border: 1px solid rgba(255,255,255,.08); border-radius: 16px; overflow: hidden; display: flex; flex-direction: column; }

  .loading { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: rgba(255,255,255,.7); gap: 16px; }
  .loading p { font-size: 13px; }
  .spinner { width: 40px; height: 40px; border: 2px solid rgba(255,255,255,.08); border-top-color: #60a5fa; border-radius: 50%; animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: rgba(255,255,255,.45); padding: 32px; }
  .empty-state svg { margin-bottom: 12px; opacity: .4; color: rgba(255,255,255,.5); }
  .empty-state p { font-size: 13px; }
  .empty-state .hint { margin-top: 12px; font-size: 11px; color: rgba(255,255,255,.35); }

  .video-toolbar { display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; border-bottom: 1px solid rgba(255,255,255,.06); flex-shrink: 0; gap: 12px; background: rgba(255,255,255,.02); }
  .video-count { font-size: 13px; color: rgba(255,255,255,.7); font-weight: 500; }
  .search-box { flex: 0 0 auto; max-width: 240px; position: relative; }
  .search-box::before { content: "🔍"; position: absolute; left: 10px; top: 50%; transform: translateY(-50%); font-size: 12px; opacity: .4; pointer-events: none; }
  .search-box input { width: 100%; padding: 8px 12px 8px 32px; border: 1px solid rgba(255,255,255,.1); border-radius: 8px; font-size: 13px; background: rgba(255,255,255,.05); color: #fff; transition: all .2s ease; }
  .search-box input:focus { outline: none; border-color: rgba(96,165,250,.4); background: rgba(255,255,255,.08); }
  .search-box input::placeholder { color: rgba(255,255,255,.35); }
  .no-results { padding: 32px 16px; text-align: center; color: rgba(255,255,255,.45); font-size: 13px; }

  .table-container { flex: 1; overflow: auto; }
  .video-table { width: 100%; border-collapse: separate; border-spacing: 0; font-size: 13px; }
  .video-table th { position: sticky; top: 0; background: rgba(255,255,255,.06); backdrop-filter: blur(10px); padding: 10px 14px; text-align: left; font-weight: 600; color: rgba(255,255,255,.75); border-bottom: 1px solid rgba(255,255,255,.06); cursor: pointer; user-select: none; white-space: nowrap; font-size: 11px; letter-spacing: .3px; text-transform: uppercase; }
  .video-table th.sortable::after { content: "⇅"; margin-left: 4px; opacity: .3; font-size: 10px; }
  .video-table th.sortable:hover::after { opacity: .6; }
  .video-table th:hover { color: #60a5fa; background: rgba(255,255,255,.08); }
  .sort-icon { margin-left: 3px; font-size: 9px; color: #60a5fa; }
  .video-table td { padding: 8px 14px; border-bottom: 1px solid rgba(255,255,255,.04); }
  .video-table tr { cursor: pointer; transition: all .15s ease; }
  .video-table tbody tr:hover { background: rgba(255,255,255,.04); }
  .col-play { width: 48px; text-align: center; }
  .col-name { min-width: 200px; }
  .col-size { width: 90px; text-align: right; color: rgba(255,255,255,.6); }
  .col-date { width: 160px; color: rgba(255,255,255,.45); }
  .col-type { width: 72px; text-align: center; }
  .play-icon { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; background: rgba(96,165,250,.15); border: 1px solid rgba(96,165,250,.3); color: #60a5fa; border-radius: 50%; transition: all .2s ease; cursor: pointer; }
  .play-icon:hover { background: rgba(96,165,250,.3); transform: scale(1.08); }
  .video-name { font-weight: 500; color: rgba(255,255,255,.9); }
  .video-ext { margin-left: 5px; color: #60a5fa; font-size: 10px; font-weight: 500; text-transform: uppercase; }
  .play-btn, .system-btn { padding: 6px 12px; font-size: 11px; font-weight: 500; cursor: pointer; border: 1px solid rgba(255,255,255,.12); border-radius: 6px; transition: all .2s ease; background: rgba(255,255,255,.06); color: rgba(255,255,255,.75); }
  .play-btn { background: rgba(96,165,250,.15); border-color: rgba(96,165,250,.3); color: #60a5fa; }
  .play-btn:hover { background: rgba(96,165,250,.3); }
  .system-btn:hover { background: rgba(255,255,255,.1); border-color: rgba(239,68,68,.3); color: #fca5a5; }
  .spinning { animation: spin 1s linear infinite; }

  .table-container::-webkit-scrollbar { width: 6px; height: 6px; }
  .table-container::-webkit-scrollbar-track { background: transparent; }
  .table-container::-webkit-scrollbar-thumb { background: rgba(255,255,255,.1); border-radius: 3px; }
  .table-container::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,.2); }
</style>
