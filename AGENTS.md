# AGENTS.md

## Quick reference

| Action | Command |
|--------|---------|
| Dev (frontend only) | `pnpm dev` |
| Dev (full Tauri app) | `pnpm tauri dev` |
| Type check | `pnpm check` |
| Build release | `pnpm tauri build`（Windows 上产出 NSIS 安装包；打包目标固定为 `nsis`） |
| Rust check | `cargo check` (in `src-tauri/`) |
| Rust test | `cargo test` (in `src-tauri/`) |
| Rust test (需先 `pnpm build`) | `cargo test -- --ignored` (in `src-tauri/`) |
| Rust lint | `cargo clippy --all-targets -- -D warnings` (in `src-tauri/`) |
| Rust format | `cargo fmt` (in `src-tauri/`) |

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`, `cargo test -- --ignored`, `pnpm check` and `pnpm build` on `windows-latest`
（Linux runner 不可用：`utils.rs`/`video.rs` 的路径与符号链接测试依赖 Windows 语义，
且编译 tauri 需要额外系统依赖）：
- `pnpm build` 末尾会执行 `scripts/check-web-build.mjs`，校验产物形状符合服务端假设
- `cargo test -- --ignored` 只跑需要真实前端产物的端到端测试，**顺序不能颠倒**（必须在 `pnpm build` 之后）

pnpm 11 起设置项只从 `pnpm-workspace.yaml` 读取（`package.json#pnpm` 字段已失效）；
esbuild 的构建脚本需在该文件的 `allowBuilds` 中显式放行，否则 `pnpm install` 会报
`ERR_PNPM_IGNORED_BUILDS` 并连带阻断 `pnpm check` / `pnpm build`。

## Architecture

Tauri 2 + SvelteKit 5 desktop app. SPA mode via `adapter-static` (no SSR).

**只有一份前端。** 桌面端（Tauri webview）与网页端（局域网浏览器）跑的是同一个
SvelteKit 构建产物（`tauri.conf.json` 的 `frontendDist`，即仓库根目录 `build/`）：
- 桌面端由 Tauri 自己加载，通过 IPC 调 Rust 命令；
- 网页端由内嵌 HTTP 服务器把同一份产物发给浏览器（`server/assets.rs`），
  数据走 `/videos` 等 JSON 接口。

两端的差异全部收敛在 `src/lib/platform/`：

| 文件 | 职责 |
|------|------|
| `platform/types.ts` | `Platform` 接口与统一的 `VideoItem` 模型 |
| `platform/desktop.ts` | IPC 实现（弹目录选择框、系统播放器、asset 协议播放） |
| `platform/web.ts` | HTTP 实现（ETag 轮询、`/refresh` + `/refresh-status`、`/video/*` 播放） |
| `platform/index.ts` | 依据 `window.__TAURI_INTERNALS__` 选择实现并导出单例 |

组件只依赖 `platform` 与 `platform.kind`（另有 `canPickFolder` / `canShare` /
`listPollIntervalMs` 三个能力标志），不得直接判断运行环境，也不得直接调用
`@tauri-apps/*`（`platform/desktop.ts` 与纯桌面组件除外）。

## Frontend (`src/`)

- `src/lib/platform/` — 运行环境抽象层（唯一区分桌面端/网页端的地方）
- `src/lib/types/` — TypeScript interfaces (VideoFile、ShareServerInfo、PasswordStatus、AppError)
- `src/lib/services/` — Tauri IPC wrappers (video.ts, share.ts, password.ts). All `invoke()` calls go through here.
- `src/lib/utils/` — format.ts (file size), qrcode.ts (dynamic import)
- `src/lib/styles/` — theme.css (design tokens as CSS variables)、buttons.css
- `src/lib/components/` — Svelte 5 components using runes (`$state`, `$derived`, `$effect`, `$props`)
- `src/routes/+page.svelte` — main page, orchestrates components only
- `src/routes/+layout.js` — disables SSR (required for Tauri)
- `scripts/check-web-build.mjs` — 构建产物形状校验（`pnpm build` 末尾执行，见下）

**Svelte 5 rules:**
- Use `$state()` not `let x = ...` for reactive state
- Use `$props()` for component inputs
- Use `$derived` / `$derived.by` for computed values
- Use `onMount` for one-time async init, not `$effect` without deps

## Backend (`src-tauri/src/`)

- `lib.rs` — Tauri Builder setup + Managed State（`AppState`：shared_videos、服务器状态机、服务器对外信息、scan/refresh/cancel 标志、ETag 缓存、前端资源来源、PasswordState）
- `commands/` — `#[tauri::command]` handlers: video.rs, share.rs, password_cmd.rs
- `server/` — embedded HTTP server: handler.rs (routing)、auth.rs、video_serve.rs (Range requests)、assets.rs (前端静态资源)、response.rs
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
- `login_template.html` 由 `include_str!` 嵌入（`server/handler.rs`，路径相对该文件）；它是**登录前**的自包含页面，不是第二套应用 UI。它的设计令牌来自 `/* {theme_tokens} */` 占位符，由 `include_str!("../../../src/lib/styles/theme.css")` 在响应时替换——**不要在模板里再写一份 `:root`**
- `server/assets.rs` 的 `FrontendAssets` 必须保持**类型擦除**（`Arc<dyn Fn>`），不得直接持有 `tauri::AppHandle` / `AssetResolver`：一旦这些类型出现在单元测试可达的位置，链接器会把整条 tao/wry 桌面栈拉进 `cargo test` 的可执行文件，它会导入 `comctl32!TaskDialogIndirect`——该符号只由带 Common Controls v6 清单的 comctl32 提供，而测试可执行文件没有清单，加载时直接 `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) 失败。只有 `run()` 的 setup 可以接触 `AppHandle`
- 网页端静态资源由 HTTP 服务器从 Tauri 内嵌的 `frontendDist` 提供：`/_app/immutable/**` 强缓存（`max-age=31536000, immutable`），`/_app/` 下其他文件 `no-store`，其余短缓存；`index.html` 与登录页为内联脚本注入**每次请求随机**的 CSP nonce，因此禁止缓存。浏览器端 CSP 不放行 `script-src 'unsafe-inline'`
- **开发模式注意**：`pnpm tauri dev` 下 webview 走 Vite dev server，但内嵌 HTTP 服务器读取的是磁盘上的 `build/`（`AssetResolver` 在 dev 模式回退到 `frontendDist` 目录）。改动前端后要让**网页端**生效，需要先执行一次 `pnpm build`
- UI 颜色一律使用 `src/lib/styles/theme.css` 的 CSS 变量（**该文件是唯一令牌来源**，桌面端组件与登录页都从它取色），**禁止硬编码颜色值**；`src/app.html` 的预涂底色是唯一有意例外（见该文件注释）
- 桌面端与网页端的能力差异只能通过 `src/lib/platform/` 表达：新增面向单一环境的功能时，先扩展 `Platform` 接口与 `canPickFolder` / `canShare` / `listPollIntervalMs` 能力标志，而不是在组件里判断环境
- `scripts/check-web-build.mjs` 断言构建产物形状满足服务端假设（内联 `<script>` 必须是裸标签以便注入 nonce、引用的资源必须存在、base 必须为空）。改动静态资源提供方式或升级 SvelteKit 后若它报错，要修的是这两者的一致性，不要绕过检查
- `src-tauri/src/server/api_tests.rs` 里有两类测试：fixtures 版（默认运行，不依赖前端构建）与 `real_build_output_is_served_end_to_end`（`#[ignore]`，CI 在 `pnpm build` 之后用 `cargo test -- --ignored` 显式运行）
- `capabilities/default.json` uses minimal permissions (`core:default`, `opener:default`, `dialog:default`) — no filesystem permissions

## Windows-specific

- `get_local_ips()` uses `if-addrs` crate to get network interfaces via system API (no subprocess, no console window)
- Paths use backslash; `getVideoSrc()`（`src/lib/services/video.ts`）在传给 `convertFileSrc` 前把反斜杠规范化为正斜杠
