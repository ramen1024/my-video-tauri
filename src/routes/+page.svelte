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
  let showPasswordModal = $state(false);
  let passwordModalMode = $state("set");
  let pinInput = $state("");
  let oldPinInput = $state("");
  let pinStep = $state(1);
  let passwordErrorMsg = $state("");
  let isPasswordSubmitting = $state(false);
  let showResetConfirm = $state(false);
  let previewPassword = $state("");

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

  async function togglePasswordProtection() {
    if (!passwordEnabled && !hasPassword) {
      openPasswordModal("set");
      return;
    }
    try {
      await invoke("set_password_enabled", { enabled: !passwordEnabled });
      passwordEnabled = !passwordEnabled;
    } catch (e) {
      errorMsg = "切换密码保护失败: " + e;
    }
  }

  function openPasswordModal(mode) {
    passwordModalMode = mode;
    pinInput = "";
    oldPinInput = "";
    pinStep = 1;
    passwordErrorMsg = "";
    previewPassword = "";
    showPasswordModal = true;
  }

  function closePasswordModal() {
    showPasswordModal = false;
    pinInput = "";
    oldPinInput = "";
    pinStep = 1;
    passwordErrorMsg = "";
    previewPassword = "";
  }

  function handleNumpadInput(digit) {
    if (isPasswordSubmitting) return;
    passwordErrorMsg = "";

    if (pinStep === 1 && passwordModalMode === "change") {
      if (oldPinInput.length < 4) {
        oldPinInput += digit;
      }
    } else {
      if (pinInput.length < 4) {
        pinInput += digit;
      }
      if (pinInput.length === 4) {
        if (passwordModalMode === "set" || passwordModalMode === "change") {
          submitPasswordSet();
        } else if (passwordModalMode === "verify") {
          submitPasswordVerify();
        }
      }
    }
  }

  function handleNumpadDelete() {
    if (isPasswordSubmitting) return;
    passwordErrorMsg = "";

    if (pinStep === 1 && passwordModalMode === "change") {
      oldPinInput = oldPinInput.slice(0, -1);
    } else {
      pinInput = pinInput.slice(0, -1);
    }
  }

  function handleNumpadClear() {
    if (isPasswordSubmitting) return;
    passwordErrorMsg = "";

    if (pinStep === 1 && passwordModalMode === "change") {
      oldPinInput = "";
    } else {
      pinInput = "";
    }
  }

  async function submitPasswordVerify() {
    isPasswordSubmitting = true;
    try {
      const result = await invoke("verify_password_cmd", { password: pinInput });
      if (result) {
        closePasswordModal();
      } else {
        passwordErrorMsg = "密码错误";
        pinInput = "";
        pinStep = 1;
      }
    } catch (e) {
      passwordErrorMsg = "验证失败: " + e;
      pinInput = "";
    } finally {
      isPasswordSubmitting = false;
    }
  }

  async function submitPasswordSet() {
    if (pinInput.length !== 4) return;

    isPasswordSubmitting = true;
    try {
      await invoke("set_password", { password: pinInput });
      hasPassword = true;
      currentPassword = pinInput;
      if (!passwordEnabled) {
        await invoke("set_password_enabled", { enabled: true });
        passwordEnabled = true;
      }
      closePasswordModal();
    } catch (e) {
      passwordErrorMsg = "设置密码失败: " + e;
      pinInput = "";
    } finally {
      isPasswordSubmitting = false;
    }
  }

  async function submitOldPassword() {
    if (oldPinInput.length !== 4) return;
    isPasswordSubmitting = true;
    try {
      const result = await invoke("verify_password_cmd", { password: oldPinInput });
      if (result) {
        pinStep = 2;
        pinInput = "";
        passwordErrorMsg = "";
      } else {
        passwordErrorMsg = "原密码错误";
        oldPinInput = "";
      }
    } catch (e) {
      passwordErrorMsg = "验证失败: " + e;
      oldPinInput = "";
    } finally {
      isPasswordSubmitting = false;
    }
  }

  async function generateRandomPwd() {
    try {
      previewPassword = await invoke("generate_random_password");
      pinInput = previewPassword;
      passwordErrorMsg = "";
    } catch (e) {
      passwordErrorMsg = "生成密码失败: " + e;
    }
  }

  async function confirmPreviewPassword() {
    if (!previewPassword) return;
    await submitPasswordSet();
  }

  async function resetPasswordAction() {
    showResetConfirm = true;
  }

  async function confirmResetPassword() {
    showResetConfirm = false;
    try {
      await invoke("reset_password");
      passwordEnabled = false;
      hasPassword = false;
      currentPassword = "";
      closePasswordModal();
    } catch (e) {
      passwordErrorMsg = "重置密码失败: " + e;
    }
  }

  function cancelResetPassword() {
    showResetConfirm = false;
  }

  $effect(() => {
    loadPasswordStatus();
  });

  let currentPinDisplay = $derived(
    pinStep === 1 && passwordModalMode === "change" ? oldPinInput : pinInput
  );

  function handlePasswordKeydown(e) {
    if (!showPasswordModal) return;
    if (e.key >= '0' && e.key <= '9') {
      handleNumpadInput(e.key);
    } else if (e.key === 'Backspace') {
      handleNumpadDelete();
    } else if (e.key === 'Escape') {
      closePasswordModal();
    }
  }
</script>

<svelte:window onkeydown={handlePasswordKeydown} />

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

  {#if showPasswordModal}
    <div class="modal-overlay" onclick={closePasswordModal}>
      <div class="modal-container" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header">
          <span class="modal-title">
            {#if passwordModalMode === "set"}
              设置密码
            {:else if passwordModalMode === "change"}
              修改密码
            {:else}
              验证密码
            {/if}
          </span>
          <button class="close-btn" onclick={closePasswordModal}>
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
          </button>
        </div>
        <div class="modal-body">
          {#if passwordModalMode === "change" && pinStep === 1}
            <div class="pin-step-label">请输入原密码</div>
          {:else if passwordModalMode === "set" || (passwordModalMode === "change" && pinStep === 2)}
            <div class="pin-step-label">请输入4位数字密码</div>
          {:else if passwordModalMode === "verify"}
            <div class="pin-step-label">请输入密码</div>
          {/if}

          <div class="pin-display">
            {#each Array(4) as _, i}
              <div class="pin-dot" class:filled={i < currentPinDisplay.length}></div>
            {/each}
          </div>

          {#if passwordErrorMsg}
            <div class="pin-error">{passwordErrorMsg}</div>
          {:else if isPasswordSubmitting}
            <div class="pin-loading">验证中...</div>
          {/if}

          <div class="numpad">
            <button class="num-key" onclick={() => handleNumpadInput("1")}>1</button>
            <button class="num-key" onclick={() => handleNumpadInput("2")}>2</button>
            <button class="num-key" onclick={() => handleNumpadInput("3")}>3</button>
            <button class="num-key" onclick={() => handleNumpadInput("4")}>4</button>
            <button class="num-key" onclick={() => handleNumpadInput("5")}>5</button>
            <button class="num-key" onclick={() => handleNumpadInput("6")}>6</button>
            <button class="num-key" onclick={() => handleNumpadInput("7")}>7</button>
            <button class="num-key" onclick={() => handleNumpadInput("8")}>8</button>
            <button class="num-key" onclick={() => handleNumpadInput("9")}>9</button>
            <button class="num-key action-key" onclick={handleNumpadClear}>清除</button>
            <button class="num-key" onclick={() => handleNumpadInput("0")}>0</button>
            <button class="num-key delete-key" onclick={handleNumpadDelete}>⌫</button>
          </div>

          {#if passwordModalMode === "change" && pinStep === 1 && oldPinInput.length === 4}
            <button class="btn btn-primary modal-submit-btn" onclick={submitOldPassword} disabled={isPasswordSubmitting}>
              {isPasswordSubmitting ? "验证中..." : "验证原密码"}
            </button>
          {/if}

          {#if (passwordModalMode === "set" || (passwordModalMode === "change" && pinStep === 2))}
            <div class="random-pwd-section">
              <button class="btn btn-secondary" onclick={generateRandomPwd} disabled={isPasswordSubmitting}>
                🎲 随机生成
              </button>
              {#if previewPassword}
                <div class="preview-pwd-box">
                  <span class="preview-pwd-value">{previewPassword}</span>
                  <button class="btn btn-sm btn-primary" onclick={confirmPreviewPassword} disabled={isPasswordSubmitting}>
                    确认使用
                  </button>
                </div>
              {/if}
            </div>
          {/if}

          {#if passwordModalMode === "change" && pinStep === 2}
            <div class="modal-actions">
              {#if showResetConfirm}
                <div class="reset-confirm">
                  <span class="reset-confirm-text">确认重置密码？</span>
                  <button class="btn btn-sm btn-danger" onclick={confirmResetPassword}>确认</button>
                  <button class="btn btn-sm" onclick={cancelResetPassword}>取消</button>
                </div>
              {:else}
                <button class="btn btn-danger" onclick={resetPasswordAction}>重置密码</button>
              {/if}
            </div>
          {/if}
        </div>
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
      <div class="share-info-header">
        <span class="share-label">🎉 局域网共享已开启</span>
        <div class="password-protection">
          <div class="protection-status" class:enabled={passwordEnabled} class:disabled={!passwordEnabled}>
            <span class="status-dot"></span>
            <span class="status-text">{passwordEnabled ? "密码保护已开启" : "密码保护未开启"}</span>
          </div>
          <button class="btn btn-sm" class:btn-protection-on={passwordEnabled} class:btn-protection-off={!passwordEnabled} onclick={togglePasswordProtection}>
            {passwordEnabled ? "关闭保护" : "开启保护"}
          </button>
          {#if passwordEnabled && currentPassword}
            <div class="current-password-display">
              <span class="pwd-label">密码:</span>
              <span class="pwd-value">{currentPassword}</span>
            </div>
          {/if}
          {#if hasPassword}
            <button class="btn btn-sm btn-manage-pwd" onclick={() => openPasswordModal("change")}>
              管理密码
            </button>
          {/if}
        </div>
      </div>
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
        <button class="btn btn-danger" onclick={cancelScanning}>取消扫描</button>
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
                  <div class="play-icon" onclick={() => playVideo(video)}>
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
    font-family: "Segoe UI", "Microsoft YaHei", -apple-system, BlinkMacSystemFont, sans-serif;
    background: #0f172a;
    color: #ffffff;
    overflow: hidden;
    min-height: 100vh;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 24px;
    position: relative;
    background: transparent;
  }

  .player-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .player-container {
    width: 90%;
    max-width: 1200px;
    background: rgba(255, 255, 255, 0.1);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.4),
      0 0 0 1px rgba(255, 255, 255, 0.1) inset;
    overflow: hidden;
  }

  .player-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 24px;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .player-title {
    font-size: 16px;
    font-weight: 500;
    color: #ffffff;
    letter-spacing: 0.5px;
  }

  .close-btn {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    color: #ffffff;
    cursor: pointer;
    padding: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.3s ease;
    border-radius: 8px;
  }

  .close-btn:hover {
    background: rgba(255, 107, 107, 0.3);
    border-color: rgba(255, 107, 107, 0.5);
    transform: scale(1.05);
  }

  .video-player {
    width: 100%;
    display: block;
    max-height: 80vh;
    background: #000;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 28px;
    background: rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 16px;
    margin-bottom: 20px;
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.3),
      0 0 0 1px rgba(255, 255, 255, 0.1) inset;
  }

  .title {
    font-size: 26px;
    font-weight: 600;
    color: #ffffff;
    letter-spacing: 0.5px;
    text-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
  }

  .actions {
    display: flex;
    gap: 12px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.3s ease;
    background: rgba(255, 255, 255, 0.1);
    color: #ffffff;
    border-radius: 12px;
    position: relative;
    overflow: hidden;
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
    transform: translateY(-2px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }

  .btn:active:not(:disabled) {
    transform: translateY(0);
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn-sm {
    padding: 6px 14px;
    font-size: 12px;
    border-radius: 8px;
    gap: 4px;
  }

  .btn-primary {
    background: linear-gradient(135deg, rgba(59, 130, 246, 0.8) 0%, rgba(37, 99, 235, 0.8) 100%);
    border-color: rgba(59, 130, 246, 0.5);
  }

  .btn-primary:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(59, 130, 246, 1) 0%, rgba(37, 99, 235, 1) 100%);
    box-shadow: 0 8px 24px rgba(59, 130, 246, 0.4);
  }

  .btn-secondary {
    background: linear-gradient(135deg, rgba(139, 92, 246, 0.8) 0%, rgba(124, 58, 237, 0.8) 100%);
    border-color: rgba(139, 92, 246, 0.5);
  }

  .btn-secondary:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(139, 92, 246, 1) 0%, rgba(124, 58, 237, 1) 100%);
    box-shadow: 0 8px 24px rgba(139, 92, 246, 0.4);
  }

  .btn-share {
    background: linear-gradient(135deg, rgba(6, 182, 212, 0.8) 0%, rgba(8, 145, 178, 0.8) 100%);
    border-color: rgba(6, 182, 212, 0.5);
  }

  .btn-share:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(6, 182, 212, 1) 0%, rgba(8, 145, 178, 1) 100%);
    box-shadow: 0 8px 24px rgba(6, 182, 212, 0.4);
  }

  .btn-danger {
    background: linear-gradient(135deg, rgba(239, 68, 68, 0.8) 0%, rgba(220, 38, 38, 0.8) 100%);
    border-color: rgba(239, 68, 68, 0.5);
  }

  .btn-danger:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(239, 68, 68, 1) 0%, rgba(220, 38, 38, 1) 100%);
    box-shadow: 0 8px 24px rgba(239, 68, 68, 0.4);
  }

  .btn-protection-on {
    background: linear-gradient(135deg, rgba(34, 197, 94, 0.8) 0%, rgba(22, 163, 74, 0.8) 100%);
    border-color: rgba(34, 197, 94, 0.5);
  }

  .btn-protection-on:hover:not(:disabled) {
    background: linear-gradient(135deg, rgba(34, 197, 94, 1) 0%, rgba(22, 163, 74, 1) 100%);
    box-shadow: 0 8px 24px rgba(34, 197, 94, 0.4);
  }

  .btn-protection-off {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .btn-protection-off:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
  }

  .btn-manage-pwd {
    background: rgba(251, 191, 36, 0.2);
    border-color: rgba(251, 191, 36, 0.4);
    color: #fbbf24;
  }

  .btn-manage-pwd:hover:not(:disabled) {
    background: rgba(251, 191, 36, 0.35);
  }

  .current-password-display {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px;
    background: rgba(96, 165, 250, 0.15);
    border: 1px solid rgba(96, 165, 250, 0.3);
    border-radius: 8px;
  }

  .pwd-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
  }

  .pwd-value {
    font-size: 18px;
    font-weight: 700;
    color: #60a5fa;
    letter-spacing: 6px;
  }

  .folder-path {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 24px;
    background: rgba(255, 255, 255, 0.06);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    margin-bottom: 16px;
    font-size: 14px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .path-label {
    color: rgba(255, 255, 255, 0.7);
    font-weight: 500;
    white-space: nowrap;
  }

  .path-value {
    color: #ffffff;
    word-break: break-all;
    font-weight: 400;
  }

  .share-info {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px;
    background: rgba(255, 255, 255, 0.06);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 16px;
    margin-bottom: 16px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .share-info-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
  }

  .share-label {
    font-weight: 600;
    color: #06b6d4;
    font-size: 14px;
    letter-spacing: 0.5px;
  }

  .password-protection {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .protection-status {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 20px;
    font-size: 12px;
    font-weight: 500;
  }

  .protection-status.enabled {
    background: rgba(34, 197, 94, 0.15);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: #4ade80;
  }

  .protection-status.disabled {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.5);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }

  .protection-status.enabled .status-dot {
    background: #4ade80;
    box-shadow: 0 0 8px rgba(74, 222, 128, 0.6);
  }

  .protection-status.disabled .status-dot {
    background: rgba(255, 255, 255, 0.3);
  }

  .firewall-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    padding: 12px 16px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 8px;
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
    background: rgba(255, 255, 255, 0.1);
    padding: 12px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }

  .share-address {
    font-size: 14px;
    color: rgba(255, 255, 255, 0.8);
    word-break: break-all;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .share-address a {
    color: #60a5fa;
    text-decoration: none;
    font-size: 15px;
    font-weight: 600;
    transition: all 0.3s ease;
  }

  .share-address a:hover {
    color: #93c5fd;
  }

  .ip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 8px 0;
  }

  .ip-btn {
    padding: 8px 16px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.3s ease;
    border-radius: 8px;
  }

  .ip-btn:hover {
    border-color: rgba(96, 165, 250, 0.4);
    color: #60a5fa;
    background: rgba(255, 255, 255, 0.12);
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

  .error-message {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
    background: rgba(239, 68, 68, 0.15);
    color: #fca5a5;
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 12px;
    margin-bottom: 16px;
    font-size: 14px;
    font-weight: 500;
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
  }

  .content {
    flex: 1;
    background: rgba(255, 255, 255, 0.04);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 16px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: rgba(255, 255, 255, 0.8);
    gap: 20px;
  }

  .loading p {
    font-size: 14px;
    letter-spacing: 0.5px;
  }

  .spinner {
    width: 48px;
    height: 48px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: #60a5fa;
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
    color: rgba(255, 255, 255, 0.5);
    padding: 40px;
  }

  .empty-state svg {
    margin-bottom: 16px;
    opacity: 0.5;
    color: rgba(255, 255, 255, 0.6);
  }

  .empty-state p {
    font-size: 14px;
  }

  .empty-state .hint {
    margin-top: 16px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
  }

  .video-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 24px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    flex-shrink: 0;
    gap: 16px;
    background: rgba(255, 255, 255, 0.03);
  }

  .video-count {
    font-size: 14px;
    color: rgba(255, 255, 255, 0.8);
    font-weight: 500;
  }

  .search-box {
    flex: 0 0 auto;
    max-width: 280px;
    position: relative;
  }

  .search-box input {
    width: 100%;
    padding: 10px 16px 10px 40px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 10px;
    font-size: 14px;
    background: rgba(255, 255, 255, 0.08);
    color: #ffffff;
    transition: all 0.3s ease;
  }

  .search-box input:focus {
    outline: none;
    border-color: rgba(96, 165, 250, 0.5);
    background: rgba(255, 255, 255, 0.12);
    box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.1);
  }

  .search-box input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  .no-results {
    padding: 40px 20px;
    text-align: center;
    color: rgba(255, 255, 255, 0.5);
    font-size: 14px;
  }

  .table-container {
    flex: 1;
    overflow: auto;
  }

  .video-table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-size: 14px;
  }

  .video-table th {
    position: sticky;
    top: 0;
    background: rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    padding: 16px 20px;
    text-align: left;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    font-size: 12px;
    letter-spacing: 0.5px;
  }

  .video-table th.sortable::after {
    content: "⇅";
    margin-left: 6px;
    opacity: 0.4;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
  }

  .video-table th.sortable:hover::after {
    opacity: 0.8;
  }

  .video-table th:hover {
    color: #60a5fa;
    background: rgba(255, 255, 255, 0.12);
  }

  .sort-icon {
    margin-left: 4px;
    font-size: 10px;
    color: #60a5fa;
  }

  .video-table td {
    padding: 14px 20px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .video-table tr {
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .video-table tbody tr:hover {
    background: rgba(255, 255, 255, 0.06);
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
    color: rgba(255, 255, 255, 0.7);
  }

  .col-date {
    width: 180px;
    color: rgba(255, 255, 255, 0.5);
  }

  .col-type {
    width: 80px;
    text-align: center;
  }

  .play-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    background: rgba(96, 165, 250, 0.2);
    border: 1px solid rgba(96, 165, 250, 0.4);
    color: #60a5fa;
    border-radius: 50%;
    transition: all 0.3s ease;
  }

  .play-icon:hover {
    background: rgba(96, 165, 250, 0.4);
    transform: scale(1.1);
  }

  .video-name {
    font-weight: 500;
    color: rgba(255, 255, 255, 0.95);
  }

  .video-ext {
    margin-left: 6px;
    color: #60a5fa;
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
  }

  .play-btn,
  .system-btn {
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    transition: all 0.3s ease;
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }

  .play-btn {
    background: rgba(96, 165, 250, 0.2);
    border-color: rgba(96, 165, 250, 0.4);
    color: #60a5fa;
  }

  .play-btn:hover {
    background: rgba(96, 165, 250, 0.4);
    transform: translateY(-1px);
  }

  .system-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(239, 68, 68, 0.5);
    color: #fca5a5;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  .table-container::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }

  .table-container::-webkit-scrollbar-track {
    background: rgba(255, 255, 255, 0.03);
    border-radius: 4px;
  }

  .table-container::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 4px;
  }

  .table-container::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.25);
  }

  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    z-index: 2000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-container {
    width: 380px;
    max-width: 95vw;
    background: rgba(15, 23, 42, 0.95);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 16px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 24px;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .modal-title {
    font-size: 18px;
    font-weight: 600;
    color: #ffffff;
  }

  .modal-body {
    padding: 24px;
  }

  .pin-step-label {
    text-align: center;
    color: rgba(255, 255, 255, 0.7);
    font-size: 14px;
    margin-bottom: 20px;
  }

  .pin-display {
    display: flex;
    justify-content: center;
    gap: 14px;
    margin-bottom: 16px;
  }

  .pin-dot {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.12);
    border: 2px solid rgba(255, 255, 255, 0.25);
    transition: all 0.2s ease;
  }

  .pin-dot.filled {
    background: #60a5fa;
    border-color: #60a5fa;
    box-shadow: 0 0 10px rgba(96, 165, 250, 0.5);
  }

  .pin-error {
    text-align: center;
    color: #fca5a5;
    font-size: 13px;
    margin-bottom: 12px;
    min-height: 20px;
  }

  .pin-loading {
    text-align: center;
    color: #60a5fa;
    font-size: 13px;
    margin-bottom: 12px;
    min-height: 20px;
  }

  .random-pwd-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .preview-pwd-box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.25);
    border-radius: 10px;
    width: 100%;
    justify-content: center;
  }

  .preview-pwd-value {
    color: #4ade80;
    font-weight: 700;
    font-size: 22px;
    letter-spacing: 8px;
  }

  .numpad {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-bottom: 12px;
  }

  .num-key {
    padding: 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.06);
    color: #ffffff;
    font-size: 20px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    user-select: none;
  }

  .num-key:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(96, 165, 250, 0.3);
  }

  .num-key:active {
    background: rgba(96, 165, 250, 0.2);
    transform: scale(0.95);
  }

  .num-key.action-key {
    font-size: 14px;
    color: rgba(255, 255, 255, 0.6);
  }

  .num-key.delete-key {
    font-size: 16px;
  }

  .num-key.delete-key:hover {
    background: rgba(239, 68, 68, 0.15);
    border-color: rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .modal-submit-btn {
    width: 100%;
    justify-content: center;
    margin-top: 4px;
  }

  .modal-actions {
    display: flex;
    justify-content: center;
    margin-top: 8px;
  }

  .modal-actions .btn {
    font-size: 12px;
    padding: 8px 16px;
  }

  .reset-confirm {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 16px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 8px;
    width: 100%;
  }

  .reset-confirm-text {
    color: #fca5a5;
    font-size: 13px;
    font-weight: 500;
  }
</style>
