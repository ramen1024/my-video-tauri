# AGENTS.md

## Quick reference

| Action | Command |
|--------|---------|
| Dev (frontend only) | `pnpm dev` |
| Dev (full Tauri app) | `pnpm tauri dev` |
| Type check | `pnpm check` |
| Build release | `pnpm tauri build` |
| Rust check | `cargo check` (in `src-tauri/`) |

No lint, formatter, or test runners are configured.

## Architecture

Tauri 2 + SvelteKit 5 desktop app. SPA mode via `adapter-static` (no SSR).
Frontend runs in Tauri webview; Rust backend provides IPC commands + embedded HTTP server for LAN sharing.

## Frontend (`src/`)

- `src/lib/types/` — TypeScript interfaces (VideoFile, ShareServerInfo, PasswordStatus)
- `src/lib/services/` — Tauri IPC wrappers (video.ts, share.ts, password.ts). All `invoke()` calls go through here.
- `src/lib/utils/` — format.ts (file size), qrcode.ts (dynamic import)
- `src/lib/components/` — Svelte 5 components using runes (`$state`, `$derived`, `$effect`, `$props`)
- `src/routes/+page.svelte` — main page, orchestrates components only
- `src/routes/+layout.js` — disables SSR (required for Tauri)

**Svelte 5 rules:**
- Use `$state()` not `let x = ...` for reactive state
- Use `$props()` for component inputs
- Use `$derived` / `$derived.by` for computed values
- Use `onMount` for one-time async init, not `$effect` without deps

## Backend (`src-tauri/src/`)

- `lib.rs` — Tauri Builder setup + Managed State（`AppState`：shared_videos、服务器状态机、scan/refresh/cancel 标志、视频缓存、ETag 缓存、PasswordState）
- `commands/` — `#[tauri::command]` handlers: video.rs, share.rs, password_cmd.rs
- `server/` — embedded HTTP server: handler.rs (routing), auth.rs, video_serve.rs (Range requests), response.rs
- `password.rs` — `PasswordState`（由 AppState 持有）：Argon2id 哈希、session、IP 限流、随机 pepper
- `video_cache.rs` — 扫描结果磁盘缓存（按文件夹维度，扫描时逐文件校验有效性）
- `logging.rs` — 双写日志（控制台 + 应用数据目录文件，5MB 轮转）
- `utils.rs` — IP detection, path sanitization, URL decoding, ETag 计算, asset scope 放行
- `models.rs` — VideoFile, ShareServerInfo structs (Serialize only)
- `error.rs` — AppError enum with `#[serde(tag, content)]` for Tauri IPC

**Adding a new Tauri command:**
1. Add function with `#[tauri::command]` in appropriate `commands/*.rs`
2. Register in `lib.rs` `generate_handler![]` macro
3. Add IPC wrapper in `src/lib/services/`

## Quirks

- Cargo.toml package name is `video-scanner`；中文名 `视频扫描器` 在 `tauri.conf.json`（productName、mainBinaryName）
- 视频流式响应在独立线程中写出，不占用 HTTP worker；并发上限 16 路（`MAX_CONCURRENT_STREAMS`），超出时退回 worker 内同步写出形成背压
- 停止服务器时 worker **并行** join，统一 5s 总超时（`SERVER_STOP_TIMEOUT_SECS`）；端口通过 unblock + Arc 归零释放
- 共享端口被占用时自动尝试下一个端口，最多 5 个（`MAX_PORT_ATTEMPTS`）
- 所有扫描入口统一经 `scan_videos_sync` 内的 `ScanGuard` 互斥（桌面扫描与网页 `/refresh` 共用，并发时返回"扫描正在进行中"）
- 扫描结果按文件夹持久化到 `video_cache.json`；缓存命中需逐文件比对（相对路径 + 大小 + 修改时间），子目录变更也会使缓存失效
- 视频列表 ETag 缓存在 AppState（按列表 Arc 指针复用），仅列表更换时重算
- `tauri.conf.json` 中 `assetProtocol.scope` 为空，扫描成功后通过 `allow_shared_folder_asset_scope` 动态放行共享文件夹（桌面端播放依赖）
- 密码 pepper 首次启动随机生成并持久化到 `password_config.json`；旧版固定 pepper 哈希验证通过后自动迁移
- HTTP 服务器校验 `Host` 头（仅允许本机 IP / localhost），新增端点无需额外处理
- Cargo uses USTC mirror (`src-tauri/.cargo/config.toml`)
- Vite dev server fixed on port 1420; HMR on 1421
- `qrcode-generator` is dynamically imported — don't add it as a top-level import
- `html_template.html` and `login_template.html` are embedded via `include_str!` in server/handler.rs — paths are relative to that file
- `capabilities/default.json` uses minimal permissions (`core:default`, `opener:default`, `dialog:default`) — no filesystem permissions

## Windows-specific

- `get_local_ips()` uses `if-addrs` crate to get network interfaces via system API (no subprocess, no console window)
- Paths use backslash; `getVideoSrc()` in frontend normalizes to forward slash for `convertFileSrc`
