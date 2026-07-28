<!--
  SharePanel 组件
  局域网共享信息面板，精简为信息条 + 二维码。
-->
<script lang="ts">
  import type { ShareServerInfo } from "$lib/types";
  import { generateQRCodeDataURL } from "$lib/utils/qrcode";
  import { openUrl } from "@tauri-apps/plugin-opener";

  interface Props {
    shareInfo: ShareServerInfo;
    onStop: () => void;
  }

  let { shareInfo, onStop }: Props = $props();

  let selectedIp = $state("");
  let qrCodeDataUrl = $state("");
  let qrAbortController: AbortController | null = null;

  $effect(() => {
    if (!selectedIp && shareInfo.ips.length > 0) {
      selectedIp = shareInfo.ips[shareInfo.ips.length - 1];
    }
  });

  $effect(() => {
    if (selectedIp && shareInfo.port) {
      if (qrAbortController) {
        qrAbortController.abort();
      }
      const controller = new AbortController();
      qrAbortController = controller;

      generateQRCodeDataURL(`http://${selectedIp}:${shareInfo.port}`)
        .then(url => {
          if (!controller.signal.aborted) {
            qrCodeDataUrl = url;
          }
        })
        .catch(e => {
          if (!controller.signal.aborted) {
            console.error("QR code generation failed:", e);
          }
        });

      return () => {
        controller.abort();
      };
    }
  });

  function selectIp(ip: string) {
    selectedIp = ip;
  }

  async function openShareUrl(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const url = `http://${selectedIp}:${shareInfo.port}`;
    try {
      await openUrl(url);
    } catch (err) {
      console.error("打开URL失败:", err);
    }
  }
</script>

<div class="share-info">
  <div class="share-main">
    {#if qrCodeDataUrl}
      <img src={qrCodeDataUrl} alt="局域网访问二维码: http://{selectedIp}:{shareInfo.port}" class="qr-code" />
    {/if}
    <div class="share-detail">
      <div class="share-status">
        <span class="status-dot"></span>
        <span>局域网共享已开启</span>
      </div>
      <div class="ip-list">
        {#each shareInfo.ips as ip}
          <button class="ip-btn" class:selected={ip === selectedIp} onclick={() => selectIp(ip)}>
            {ip}
          </button>
        {/each}
      </div>
      <button class="selected-link" onclick={openShareUrl}>
        http://{selectedIp}:{shareInfo.port}
      </button>
    </div>
  </div>
  <div class="share-footer">
    <span class="firewall-hint">其他设备无法访问时，请在 Windows 防火墙中允许端口 {shareInfo.port}</span>
    <button class="btn btn-danger" onclick={onStop}>停止共享</button>
  </div>
</div>

<style>
  .share-info {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    background: #111827;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 12px;
    margin-bottom: 12px;
  }

  .share-main {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }

  .qr-code {
    width: 96px;
    height: 96px;
    background: #ffffff;
    padding: 6px;
    border-radius: 8px;
    flex-shrink: 0;
  }

  .share-detail {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .share-status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: #10b981;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #10b981;
  }

  .ip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .ip-btn {
    padding: 5px 10px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
    border-radius: 6px;
  }

  .ip-btn:hover {
    border-color: rgba(59, 130, 246, 0.3);
    color: rgba(255, 255, 255, 0.85);
  }

  .ip-btn.selected {
    border-color: rgba(59, 130, 246, 0.5);
    background: rgba(59, 130, 246, 0.12);
    color: #60a5fa;
  }

  .selected-link {
    color: #60a5fa;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    background: none;
    border: none;
    padding: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    text-align: left;
    transition: color 0.15s ease;
  }

  .selected-link:hover {
    color: #93c5fd;
  }

  .share-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .firewall-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
  }

  @media (max-width: 640px) {
    .share-main {
      flex-direction: column;
      align-items: flex-start;
    }

    .share-footer {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
