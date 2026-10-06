# AGENTS.md

## Quick reference

| Action | Command |
|--------|---------|
| Dev (frontend only) | `pnpm dev` |
| Dev (full Tauri app) | `pnpm tauri dev` |
| Type check | `pnpm check` |
| Build release | `pnpm tauri build` |
| Rust check | `cargo check` (in `src-tauri/`) |
| Rust test | `cargo test` (in `src-tauri/`) |
| Rust lint | `cargo clippy --all-targets -- -D warnings` (in `src-tauri/`) |
| Rust format | `cargo fmt` (in `src-tauri/`) |

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test`, `pnpm check` and `pnpm build` on `windows-latest`（Linux runner 不可用：
`utils.rs`/`video.rs` 的路径与符号链接测试依赖 Windows 语义，且编译 tauri 需要额外系统依赖）。

pnpm 11 起设置项只从 `pnpm-workspace.yaml` 读取（`package.json#pnpm` 字段已失效）；
esbuild 的构建脚本需在该文件的 `allowBuilds` 中显式放行，否则 `pnpm install` 会报
`ERR_PNPM_IGNORED_BUILDS` 并连带阻断 `pnpm check` / `pnpm build`。

## Architecture

Tauri 2 + SvelteKit 5 desktop app. SPA mode via `adapter-static` (no SSR).
Frontend runs in Tauri webview; Rust backend provides IPC commands + embedded HTTP server for LAN sharing.

## Frontend (`src/`)

- `src/lib/types/` — TypeScript interfaces (VideoFile, ShareServerInfo, PasswordStatus)
- `src/lib/services/` — Tauri IPC wrappers (video.ts, share.ts, password.ts). All `invoke()` calls go through here.
- `src/lib/utils/` — format.ts (file size), qrcode.ts (dynamic import)
- `src/lib/styles/` — theme.css (design tokens as CSS variables), buttons.css
- `src/lib/components/` — Svelte 5 components using runes (`$state`, `$derived`, `$effect`, `$props`)
- `src/routes/+page.svelte` — main page, orchestrates components only
- `src/routes/+layout.js` — disables SSR (required for Tauri)

**Svelte 5 rules:**
- Use `$state()` not `let x = ...` for reactive state
- Use `$props()` for component inputs
- Use `$derived` / `$derived.by` for computed values
- Use `onMount` for one-time async init, not `$effect` without deps

## Backend (`src-tauri/src/`)

- `lib.rs` — Tauri Builder setup + Managed State（`AppState`：shared_videos、服务器状态机、服务器对外信息、scan/refresh/cancel 标志、ETag 缓存、PasswordState）
- `commands/` — `#[tauri::command]` handlers: video.rs, share.rs, password_cmd.rs
- `server/` — embedded HTTP server: handler.rs (routing), auth.rs, video_serve.rs (Range requests), response.rs
- `password.rs` — `PasswordState`（由 AppState 持有）：Argon2id 哈希、session、IP 限流、随机 pepper
- `logging.rs` — 双写日志（控制台 + 应用数据目录文件，5MB 轮转）
- `utils.rs` — IP detection, path sanitization, URL decoding, ETag 计算, asset scope 放行
- `models.rs` — VideoFile / ShareServerInfo / ShareStatus（IPC）、VideoSummary（HTTP 响应，不含 `path`）
- `error.rs` — AppError enum with `#[serde(tag, content)]` for Tauri IPC

**Adding a new Tauri command:**
1. Add function with `#[tauri::command]` in appropriate `commands/*.rs`
2. Register in `lib.rs` `generate_handler![]` macro
3. Add IPC wrapper in `src/lib/services/`

## Quirks

- Cargo.toml package name is `video-scanner`；中文名 `视频扫描器` 在 `tauri.conf.json`（productName、mainBinaryName）
- 视频流式响应在独立线程中写出，不占用 HTTP worker；并发上限 16 路（`MAX_CONCURRENT_STREAMS`），超出时退回 worker 内同步写出形成背压
- **worker 的存活只由 `server::StopSignal` 决定，绝不读 `AppState::ServerState`**：`start_http_server` 启动 worker 时状态仍是 `Starting`，`Running` 要等调用方 `set_server_running` 之后才写入。若 worker 用状态机判断自己是否该工作，就会在这段窗口内集体退出——端口仍在监听却无人处理请求，客户端只能一直挂起。每次启动新建一个 `StopSignal`，旧实例的停止不影响新实例
- 停止服务器先置 `StopSignal`（worker 在循环顶判断），再按 worker 数量 `unblock()` 唤醒阻塞在 `recv` 的 worker（`unblock()` 以哨兵入队，没有线程在等也不会丢失）；随后 worker **并行** join，统一 5s 总超时（`SERVER_STOP_TIMEOUT_SECS`）
- 端口被占用时自动尝试下一个端口，最多 5 个（`MAX_PORT_ATTEMPTS`）；每次尝试用独立的 `StopSignal`，等待启动超时后立即置位，避免迟到的成功实例留下占着端口的孤儿 worker
- 所有扫描入口统一经 `scan_videos_sync` 内的 `ScanGuard` 互斥（桌面扫描与网页 `/refresh` 共用，并发时返回"扫描正在进行中"）
- 每次扫描都完整遍历目录并读取每文件元数据，**扫描结果不落盘缓存**（v0.3.4 及更早版本的 `video_cache.json` 已移除：判定缓存有效性本身就要遍历目录，缓存只省下排序与写盘，收益不抵一处额外的磁盘 IO 与失效风险）
- 视频列表 ETag 缓存在 AppState（按列表 Arc 指针复用），仅列表更换时重算
- HTTP `GET /videos` 序列化的是 `VideoSummary`（不含本机绝对路径 `path`）；`VideoFile` 仅用于桌面端 IPC，改动时不要混用
- webview 重载会清空前端状态，`+page.svelte` 的 `onMount` 通过 `get_share_status` 恢复共享状态与文件列表（否则服务器仍在运行却无法停止）
- `/refresh-status` 以 `pending` 字段表示"本次刷新尚无结果"，前端不得依赖 `message` 文案判断
- `AppState` 的加锁顺序统一为 handle → threads → state → share_info，新增方法需保持一致，避免 ABBA 死锁
- `tauri.conf.json` 中 `assetProtocol.scope` 为空，扫描成功后通过 `allow_shared_folder_asset_scope` 动态放行共享文件夹（桌面端播放依赖）
- 密码 pepper 首次启动随机生成并持久化到 `password_config.json`；旧版固定 pepper 哈希验证通过后自动迁移
- HTTP 服务器校验 `Host` 头（仅允许本机 IP / localhost），新增端点无需额外处理
- Cargo uses USTC mirror (`src-tauri/.cargo/config.toml`)
- Vite dev server fixed on port 1420; HMR on 1421
- `qrcode-generator` is dynamically imported — don't add it as a top-level import
- `html_template.html` and `login_template.html` are embedded via `include_str!` in server/handler.rs — paths are relative to that file
- UI 颜色一律使用 `src/lib/styles/theme.css` 的 CSS 变量（命名与网页端模板的 `:root` 对齐），**禁止硬编码颜色值**；新增组件直接从令牌取色
- `capabilities/default.json` uses minimal permissions (`core:default`, `opener:default`, `dialog:default`) — no filesystem permissions

## Windows-specific

- `get_local_ips()` uses `if-addrs` crate to get network interfaces via system API (no subprocess, no console window)
- Paths use backslash; `getVideoSrc()` in frontend normalizes to forward slash for `convertFileSrc`
