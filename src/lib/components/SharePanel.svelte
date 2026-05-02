<script lang="ts">
  import type { ShareServerInfo } from "$lib/types";
  import { generateQRCodeDataURL } from "$lib/utils/qrcode";

  interface Props {
    shareInfo: ShareServerInfo;
    onStop: () => void;
  }

  let { shareInfo, onStop }: Props = $props();

  let selectedIp = $state("");
  let qrCodeDataUrl = $state("");

  $effect(() => {
    if (!selectedIp && shareInfo.ips.length > 0) {
      selectedIp = shareInfo.ips[shareInfo.ips.length - 1];
    }
  });

  $effect(() => {
    if (selectedIp && shareInfo.port) {
      generateQRCodeDataURL(`http://${selectedIp}:${shareInfo.port}`)
        .then(url => { qrCodeDataUrl = url; })
        .catch(e => { console.error("QR code generation failed:", e); });
    }
  });

  function selectIp(ip: string) {
    selectedIp = ip;
  }
</script>

<div class="share-info">
  <div class="share-info-header">
    <span class="share-label">局域网共享已开启</span>
    <button class="btn btn-danger" onclick={onStop}>
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect></svg>
      停止共享
    </button>
  </div>

  <div class="share-content">
    {#if qrCodeDataUrl}
      <img src={qrCodeDataUrl} alt="局域网访问二维码: http://{selectedIp}:{shareInfo.port}" class="qr-code" />
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
    如果其他设备无法访问，请在 Windows 防火墙中添加入站规则允许端口 {shareInfo.port}
  </div>
</div>

<style>
  .share-info {
    display: flex; flex-direction: column; gap: 12px; padding: 16px;
    background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.08);
    border-radius: 16px; margin-bottom: 12px;
  }
  .share-info-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 10px; }
  .share-label { font-weight: 600; color: #22d3ee; font-size: 13px; }
  .share-content { display: flex; align-items: flex-start; gap: 16px; margin-top: 8px; }
  .qr-code {
    width: 112px; height: 112px; background: rgba(255,255,255,.06);
    padding: 10px; border-radius: 10px; border: 1px solid rgba(255,255,255,.12);
  }
  .share-address {
    font-size: 13px; color: rgba(255,255,255,.7); word-break: break-all;
    display: flex; flex-direction: column; gap: 6px;
  }
  .share-address a { color: #60a5fa; text-decoration: none; font-size: 14px; font-weight: 600; transition: all .2s ease; }
  .share-address a:hover { color: #93c5fd; }
  .ip-list { display: flex; flex-wrap: wrap; gap: 8px; margin: 4px 0; }
  .ip-btn {
    padding: 6px 12px; border: 1px solid rgba(255,255,255,.1);
    background: rgba(255,255,255,.05); color: rgba(255,255,255,.6);
    font-size: 12px; font-weight: 500; cursor: pointer; transition: all .2s ease; border-radius: 8px;
  }
  .ip-btn:hover { border-color: rgba(96,165,250,.3); color: #60a5fa; background: rgba(255,255,255,.08); }
  .ip-btn.selected { border-color: #fbbf24; color: #fbbf24; background: rgba(251,191,36,.08); }
  .selected-link { font-size: 14px; font-weight: 600; }
  .firewall-hint {
    font-size: 12px; color: rgba(255,255,255,.5); padding: 10px 14px;
    background: rgba(239,68,68,.06); border: 1px solid rgba(239,68,68,.12); border-radius: 8px;
  }
  .btn {
    display: flex; align-items: center; gap: 6px; padding: 10px 18px;
    border: 1px solid rgba(255,255,255,.15); font-size: 13px; font-weight: 500;
    cursor: pointer; transition: all .2s ease; background: rgba(255,255,255,.08);
    color: #fff; border-radius: 10px;
  }
  .btn:hover:not(:disabled) { background: rgba(255,255,255,.14); transform: translateY(-1px); }
  .btn-danger { background: rgba(239,68,68,.2); border-color: rgba(239,68,68,.4); color: #fca5a5; }
  .btn-danger:hover:not(:disabled) { background: rgba(239,68,68,.35); color: #fecaca; }
</style>
