<script lang="ts">
  import { onMount } from "svelte";
  import type { VideoFile } from "$lib/types";
  import { getVideoSrc } from "$lib/services/video";

  interface Props {
    video: VideoFile;
    onClose: () => void;
  }

  let { video, onClose }: Props = $props();

  let videoElement: HTMLVideoElement | undefined = $state();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  onMount(() => {
    return () => {
      if (videoElement) {
        videoElement.pause();
        videoElement.src = "";
        videoElement.load();
      }
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="player-overlay" role="dialog" aria-modal="true" aria-label="视频播放器">
  <div class="player-container">
    <div class="player-header">
      <span class="player-title">{video.name}</span>
      <button class="close-btn" onclick={onClose} aria-label="关闭播放器">
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
    </div>
    <video bind:this={videoElement} src={getVideoSrc(video.path)} controls autoplay class="video-player">您的浏览器不支持视频播放</video>
  </div>
</div>

<style>
  .player-overlay {
    position: fixed; top: 0; left: 0; right: 0; bottom: 0;
    background: rgba(0,0,0,.6); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px);
    z-index: 1000; display: flex; align-items: center; justify-content: center;
  }
  .player-container {
    width: 90%; max-width: 1200px;
    background: rgba(255,255,255,.1); backdrop-filter: blur(20px);
    border: 1px solid rgba(255,255,255,.2); border-radius: 16px;
    box-shadow: 0 8px 32px rgba(0,0,0,.4); overflow: hidden;
  }
  .player-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 20px; background: rgba(255,255,255,.05);
    border-bottom: 1px solid rgba(255,255,255,.1);
  }
  .player-title { font-size: 15px; font-weight: 500; color: #fff; }
  .close-btn {
    background: rgba(255,255,255,.1); border: 1px solid rgba(255,255,255,.2);
    color: #fff; cursor: pointer; padding: 8px; display: flex; align-items: center;
    justify-content: center; transition: all .2s ease; border-radius: 8px;
  }
  .close-btn:hover { background: rgba(239,68,68,.3); border-color: rgba(239,68,68,.5); }
  .video-player { width: 100%; display: block; max-height: 80vh; background: #000; }
</style>
