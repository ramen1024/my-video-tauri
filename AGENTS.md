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

- `lib.rs` — Tauri Builder setup + Managed State（`AppState`：shared_videos、服务器状态机、scan/refresh/cancel 标志、视频缓存）
- `commands/` — `#[tauri::command]` handlers: video.rs, share.rs, password_cmd.rs
- `server/` — embedded HTTP server: handler.rs (routing), auth.rs, video_serve.rs (Range requests), response.rs
- `password.rs` — password storage, sessions, rate limiting
- `utils.rs` — IP detection, path sanitization, URL decoding
- `models.rs` — VideoFile, ShareServerInfo structs (Serialize only)
- `error.rs` — AppError enum with `#[serde(tag, content)]` for Tauri IPC

**Adding a new Tauri command:**
1. Add function with `#[tauri::command]` in appropriate `commands/*.rs`
2. Register in `lib.rs` `generate_handler![]` macro
3. Add IPC wrapper in `src/lib/services/`

## Quirks

- Cargo.toml package name is `video-scanner`；中文名 `视频扫描器` 在 `tauri.conf.json`（productName、mainBinaryName）
- 视频流式响应在独立线程中写出，不占用 HTTP worker；停止服务器时 worker `join` 带 5s 超时
- 所有扫描入口统一经 `scan_videos_sync` 内的 `ScanGuard` 互斥（桌面扫描与网页 `/refresh` 共用，并发时返回"扫描正在进行中"）
- HTTP 服务器校验 `Host` 头（仅允许本机 IP / localhost），新增端点无需额外处理
- Cargo uses USTC mirror (`src-tauri/.cargo/config.toml`)
- Vite dev server fixed on port 1420; HMR on 1421
- `qrcode-generator` is dynamically imported — don't add it as a top-level import
- `html_template.html` and `login_template.html` are embedded via `include_str!` in server/handler.rs — paths are relative to that file
- `capabilities/default.json` uses minimal permissions (`core:default`, `opener:default`, `dialog:default`) — no filesystem permissions

## Windows-specific

- `get_local_ips()` uses `if-addrs` crate to get network interfaces via system API (no subprocess, no console window)
- Paths use backslash; `getVideoSrc()` in frontend normalizes to forward slash for `convertFileSrc`
