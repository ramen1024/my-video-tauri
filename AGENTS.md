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

- `lib.rs` — Tauri Builder setup + global statics (SHARED_VIDEOS, SERVER_RUNNING, etc.)
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

- `Cargo.toml` uses Chinese package/bin name `视频扫描器` — may cause encoding issues in some CI
- Cargo uses USTC mirror (`src-tauri/.cargo/config.toml`)
- Vite dev server fixed on port 1420; HMR on 1421
- `qrcode-generator` is dynamically imported — don't add it as a top-level import
- `html_template.html` and `login_template.html` are embedded via `include_str!` in server/handler.rs — paths are relative to that file
- `capabilities/default.json` has broad `fs:allow-read-file` and `shell:allow-execute` permissions — be cautious when adding new capabilities

## Windows-specific

- `get_local_ips()` parses `ipconfig` output — handles both Chinese ("IPv4 地址") and English ("IPv4 Address") locales
- Paths use backslash; `getVideoSrc()` in frontend normalizes to forward slash for `convertFileSrc`
