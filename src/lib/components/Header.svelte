<!--
  Header 组件
  应用顶部导航栏，包含选择文件夹、刷新扫描、开启/停止共享等操作按钮
-->
<script lang="ts">
  interface Props {
    isScanning: boolean;
    isSharing: boolean;
    currentFolder: string;
    onSelectFolder: () => void;
    onScan: () => void;
    onStartShare: () => void;
    onStopShare: () => void;
    isStartingShare: boolean;
  }

  let { isScanning, isSharing, currentFolder, onSelectFolder, onScan, onStartShare, onStopShare, isStartingShare }: Props = $props();
</script>

<header class="header">
  <h1 class="title">视频扫描器</h1>
  <div class="actions">
    <button class="btn btn-primary" onclick={onSelectFolder}>
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
      选择文件夹
    </button>
    {#if currentFolder}
      <button class="btn btn-secondary" onclick={onScan} disabled={isScanning}>
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class:spinning={isScanning}><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"></path><path d="M21 3v5h-5"></path></svg>
        刷新
      </button>
      {#if isSharing}
        <button class="btn btn-danger" onclick={onStopShare}>
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect></svg>
          停止共享
        </button>
      {:else}
        <button class="btn btn-share" onclick={onStartShare} disabled={isStartingShare}>
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="5" r="3"></circle><circle cx="6" cy="12" r="3"></circle><circle cx="18" cy="19" r="3"></circle><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"></line><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"></line></svg>
          {isStartingShare ? "开启中..." : "局域网共享"}
        </button>
      {/if}
    {/if}
  </div>
</header>

<style>
  .header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 16px 20px; background: rgba(255,255,255,.06);
    backdrop-filter: blur(20px); border: 1px solid rgba(255,255,255,.1);
    border-radius: 16px; margin-bottom: 12px; box-shadow: 0 4px 20px rgba(0,0,0,.2);
  }
  .title { font-size: 22px; font-weight: 600; color: #fff; letter-spacing: .3px; }
  .actions { display: flex; gap: 8px; }
  .spinning { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
