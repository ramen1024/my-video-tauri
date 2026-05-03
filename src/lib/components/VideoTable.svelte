<script lang="ts">
  import { onDestroy } from "svelte";
  import type { VideoFile, SortField, SortDirection } from "$lib/types";
  import { formatFileSize, isSupportedFormat } from "$lib/utils/format";

  interface Props {
    videos: VideoFile[];
    onPlay: (video: VideoFile) => void;
  }

  let { videos, onPlay }: Props = $props();

  let sortField = $state<SortField>("name");
  let sortDirection = $state<SortDirection>("asc");
  let searchTerm = $state("");
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let debouncedSearch = $state("");

  function handleSearchInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    searchTerm = value;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { debouncedSearch = value; }, 200);
  }

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  function toggleSort(field: SortField) {
    if (sortField === field) {
      sortDirection = sortDirection === "asc" ? "desc" : "asc";
    } else {
      sortField = field;
      sortDirection = "asc";
    }
  }

  let displayVideos = $derived.by(() => {
    let list = [...videos];
    if (debouncedSearch.trim()) {
      const term = debouncedSearch.toLowerCase();
      list = list.filter(v => v.name.toLowerCase().includes(term));
    }
    list.sort((a, b) => {
      let valA: string | number = a[sortField] ?? "";
      let valB: string | number = b[sortField] ?? "";
      if (sortField === "size") { valA = Number(valA); valB = Number(valB); }
      else if (sortField === "modified") { valA = valA || ""; valB = valB || ""; }
      else { valA = String(valA).toLowerCase(); valB = String(valB).toLowerCase(); }
      if (valA < valB) return sortDirection === "asc" ? -1 : 1;
      if (valA > valB) return sortDirection === "asc" ? 1 : -1;
      return 0;
    });
    return list;
  });
</script>

<div class="video-toolbar">
  <div class="video-count">共找到 {videos.length} 个视频文件</div>
  <div class="search-box">
    <input type="text" placeholder="搜索视频..." value={searchTerm} oninput={handleSearchInput} aria-label="搜索视频" />
  </div>
</div>

{#if displayVideos.length === 0 && debouncedSearch}
  <div class="no-results"><p>没有找到匹配 "{debouncedSearch}" 的视频</p></div>
{:else}
  <div class="table-container">
    <table class="video-table">
      <thead>
        <tr>
          <th class="col-play"></th>
          <th class="col-name">
            <button class="sort-btn" onclick={() => toggleSort("name")}>
              文件名 {#if sortField === "name"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
            </button>
          </th>
          <th class="col-size">
            <button class="sort-btn" onclick={() => toggleSort("size")}>
              大小 {#if sortField === "size"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
            </button>
          </th>
          <th class="col-date">
            <button class="sort-btn" onclick={() => toggleSort("modified")}>
              日期 {#if sortField === "modified"}<span class="sort-icon">{sortDirection === "asc" ? "▲" : "▼"}</span>{/if}
            </button>
          </th>
          <th class="col-type">播放</th>
        </tr>
      </thead>
      <tbody>
        {#each displayVideos as video (video.path)}
          <tr>
            <td class="col-play">
              <button class="play-icon" onclick={() => onPlay(video)} aria-label="播放视频">
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
                <button class="play-btn" onclick={() => onPlay(video)}>播放</button>
              {:else}
                <button class="system-btn" onclick={() => onPlay(video)}>系统</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .video-toolbar {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 16px; border-bottom: 1px solid rgba(255,255,255,.06);
    flex-shrink: 0; gap: 12px; background: rgba(255,255,255,.02);
  }
  .video-count { font-size: 13px; color: rgba(255,255,255,.7); font-weight: 500; }
  .search-box { flex: 0 0 auto; max-width: 240px; position: relative; }
  .search-box::before {
    content: "🔍"; position: absolute; left: 10px; top: 50%;
    transform: translateY(-50%); font-size: 12px; opacity: .4; pointer-events: none;
  }
  .search-box input {
    width: 100%; padding: 8px 12px 8px 32px;
    border: 1px solid rgba(255,255,255,.1); border-radius: 8px;
    font-size: 13px; background: rgba(255,255,255,.05); color: #fff;
    transition: all .2s ease;
  }
  .search-box input:focus { outline: none; border-color: rgba(96,165,250,.4); background: rgba(255,255,255,.08); }
  .search-box input::placeholder { color: rgba(255,255,255,.35); }
  .no-results { padding: 32px 16px; text-align: center; color: rgba(255,255,255,.45); font-size: 13px; }

  .table-container { flex: 1; overflow: auto; }
  .video-table { width: 100%; border-collapse: separate; border-spacing: 0; font-size: 13px; }
  .video-table th {
    position: sticky; top: 0; background: rgba(255,255,255,.06);
    backdrop-filter: blur(10px); padding: 10px 14px; text-align: left;
    font-weight: 600; color: rgba(255,255,255,.75);
    border-bottom: 1px solid rgba(255,255,255,.06);
    font-size: 11px; letter-spacing: .3px; text-transform: uppercase;
  }
  .sort-btn {
    background: none; border: none; color: inherit; cursor: pointer;
    font: inherit; padding: 0; display: inline-flex; align-items: center; gap: 4px;
    transition: color .2s ease;
  }
  .sort-btn:hover { color: #60a5fa; }
  .sort-icon { font-size: 9px; color: #60a5fa; }
  .video-table td { padding: 8px 14px; border-bottom: 1px solid rgba(255,255,255,.04); }
  .video-table tr { cursor: pointer; transition: all .15s ease; }
  .video-table tbody tr:hover { background: rgba(255,255,255,.04); }
  .col-play { width: 48px; text-align: center; }
  .col-name { min-width: 200px; }
  .col-size { width: 90px; text-align: right; color: rgba(255,255,255,.6); }
  .col-date { width: 160px; color: rgba(255,255,255,.45); }
  .col-type { width: 72px; text-align: center; }
  .play-icon {
    display: inline-flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; background: rgba(96,165,250,.15);
    border: 1px solid rgba(96,165,250,.3); color: #60a5fa;
    border-radius: 50%; transition: all .2s ease; cursor: pointer;
  }
  .play-icon:hover { background: rgba(96,165,250,.3); transform: scale(1.08); }
  .video-name { font-weight: 500; color: rgba(255,255,255,.9); }
  .video-ext { margin-left: 5px; color: #60a5fa; font-size: 10px; font-weight: 500; text-transform: uppercase; }
  .play-btn, .system-btn {
    padding: 6px 12px; font-size: 11px; font-weight: 500; cursor: pointer;
    border: 1px solid rgba(255,255,255,.12); border-radius: 6px;
    transition: all .2s ease; background: rgba(255,255,255,.06); color: rgba(255,255,255,.75);
  }
  .play-btn { background: rgba(96,165,250,.15); border-color: rgba(96,165,250,.3); color: #60a5fa; }
  .play-btn:hover { background: rgba(96,165,250,.3); }
  .system-btn:hover { background: rgba(255,255,255,.1); border-color: rgba(239,68,68,.3); color: #fca5a5; }

  .table-container::-webkit-scrollbar { width: 6px; height: 6px; }
  .table-container::-webkit-scrollbar-track { background: transparent; }
  .table-container::-webkit-scrollbar-thumb { background: rgba(255,255,255,.1); border-radius: 3px; }
  .table-container::-webkit-scrollbar-thumb:hover { background: rgba(255,255,255,.2); }
</style>
