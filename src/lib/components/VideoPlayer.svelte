<!--
  VideoPlayer 组件
  全屏视频播放器，背景压暗，让视频成为绝对视觉焦点。
-->
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
  let containerElement: HTMLDivElement | undefined = $state();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  onMount(() => {
    if (containerElement) containerElement.focus();
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

<div class="player-overlay" role="dialog" aria-modal="true" aria-label="视频播放器" tabindex="-1" bind:this={containerElement} onclick={onClose} onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}>
  <div class="player-container" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
    <div class="player-header">
      <span class="player-title">{video.name}</span>
      <button class="close-btn" onclick={onClose} aria-label="关闭播放器">
        <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
    </div>
    <video bind:this={videoElement} src={getVideoSrc(video.path)} controls autoplay class="video-player">
      <track kind="captions" />
      您的浏览器不支持视频播放
    </video>
  </div>
</div>

<style>
  .player-overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    outline: none;
    padding: 24px;
  }

  .player-container {
    width: 100%;
    max-width: 1200px;
    background: var(--bg);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: var(--shadow-lg);
  }

  .player-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .player-title {
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    margin-right: 16px;
  }

  .close-btn {
    background: var(--surface-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-secondary);
    cursor: pointer;
    padding: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background-color 0.15s ease, color 0.15s ease;
    border-radius: var(--radius-sm);
  }

  .close-btn:hover {
    background: var(--danger-hover);
    color: var(--danger);
  }

  .video-player {
    width: 100%;
    display: block;
    max-height: calc(100vh - 140px);
    background: var(--black);
  }
</style>
