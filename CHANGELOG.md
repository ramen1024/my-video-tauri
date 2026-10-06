# 更新日志

本项目所有重要变更记录于此，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

## 未发布（v0.3.5）

### 结构：合并为单一前端

- **删除第二套网页端实现**：原先网页端是内联在 `html_template.html` 里的独立页面（约 490 行 CSS + 337 行 JS），与 Svelte 桌面端重复实现列表渲染、排序搜索、播放器、错误提示，且已经出现行为漂移（同一份"格式化文件大小"两个版本，网页版缺单位下标保护；设计令牌两份 `:root` 各写一遍）。现在网页端直接加载桌面端同一份 SvelteKit 构建产物，由内嵌 HTTP 服务器（新增 `server/assets.rs`）提供
- 新增 `src/lib/platform/` 运行环境抽象层：一份 `Platform` 接口 + `desktop.ts`（Tauri IPC）与 `web.ts`（HTTP 接口）两个实现，组件只依赖接口与 `canPickFolder` / `canShare` / `listPollIntervalMs` 能力标志
- 静态资源按 SvelteKit 产物约定区分缓存：`_app/immutable/**` 强缓存，`_app/` 下其他文件 `no-store`，页面 `no-store`
- 页面与登录页的内联脚本改为注入**每次请求随机**的 CSP nonce，浏览器端 CSP 不再放行 `script-src 'unsafe-inline'`
- 设计令牌收敛为单一来源：登录页原来自带的第二份 `:root` 改为响应时注入 `src/lib/styles/theme.css`，并补齐模板实际用到的 `--warning` 令牌
- 新增 `scripts/check-web-build.mjs`（`pnpm build` 末尾执行）：断言产物形状满足服务端假设——内联 `<script>` 必须是裸标签（否则 nonce 注入失效导致白屏）、引用的资源必须存在、base 必须为空

### 安全

- **`GET /video/*` 补上扩展名白名单**：此前只校验路径位于共享文件夹内，导致该端点退化成"共享目录的通用文件下载器"——目录里的 `.txt`/`.db`/配置等任意文件都能被局域网客户端取走（桌面端的 `play_video` 一直是校验扩展名的）。现在非视频扩展名返回 403
- 网页端静态资源纳入认证门；未认证请求 `/_app/*` 会得到重定向而不是资源内容

### 修复

- 修复 HTTP 服务器 worker 启动竞态导致的"共享开了但页面打不开"：worker 原先用 `AppState::ServerState` 判断是否继续工作，而该状态要等 `start_http_server` 返回、调用方执行 `set_server_running` 之后才变为 `Running`，worker 在启动窗口内看到的必然是 `Starting`，于是全部立刻退出——端口仍在监听却没有任何线程处理请求，客户端连接后一直挂起。改为每次启动新建 `server::StopSignal`，worker 只依据它退出，与状态机彻底解耦；并新增 5 个测试覆盖该窗口（含"状态停在 `Starting` 时必须仍能服务请求"的回归测试）
- 端口自增重试期间，某次尝试等待启动超时后立即置位该次尝试的 `StopSignal`，避免迟到的成功实例留下无人跟踪却占着端口的孤儿 worker
- 日志轮转改为**每次写入后**检查大小，不再只在启动时检查一次：长跑会话的 `video-scanner.log` 此前会无上限增长，与文档承诺的"超 5MB 轮转"不符
- 停止共享、启动共享增加进行中状态；选择文件夹在扫描期间禁用，避免并发扫描把界面置于"文件夹标签与列表不一致"的死角

### 测试

- 新增 `server/api_tests.rs`：用裸 TCP 对真实 tiny_http 服务器做端到端请求，覆盖此前零测试的请求编排层——Host 校验（403）、登录门与 `/login` 语义、SPA 与静态资源路由及缓存头、CSP nonce 注入与页面一致性、`/videos` 的 ETag/304 与"不泄露绝对路径"、`/video/*` 的扩展名与穿越防护、`/auth` 的 cookie 属性
- 其中 `real_build_output_is_served_end_to_end` 用真实 SvelteKit 产物验证，标记 `#[ignore]` 并由 CI 在 `pnpm build` 之后以 `cargo test -- --ignored` 显式运行（而非静默跳过）
- 新增日志轮转测试（启动时与运行时两条路径）
- 移除扫描结果的磁盘缓存（`video_cache.json`）：判定缓存有效性必须先完整遍历目录读元数据，命中时省下的只有排序与写盘，收益不抵一处额外磁盘 IO 与缓存损坏静默失效的风险；同时去掉 `use_cache` 参数、`ScannedFile` 中间结构与相关测试
- 补充扫描过滤测试（扩展名、最小体积、子目录变更）与根目录拒绝测试

### 构建与文档

- `tauri.conf.json` 的 `bundle.targets` 由 `["app"]` 改为 `["nsis"]`，`pnpm tauri build` 现在在 Windows 上真正产出安装包（`app` 是 macOS 专用包类型，此前等于不打包，与 README 的描述不符）。选择 `nsis` 而非框架默认目标集，是因为默认会先尝试 MSI（WiX）并在本机 `light.exe` 阶段失败、中断整个打包，而 `nsis` 已实测产出可用安装包；macOS / Linux 打包请用 `--bundles` 覆盖
- 修正文档与实现不符之处：README 的缓存/轮转/安装包描述、`docs/api.md` 补齐静态资源路由、Host 校验、`/login` 语义、`/auth` 各错误码、`/video/*` 扩展名规则、端口自增说明
- `AGENTS.md` 的 clippy 命令统一为 `--all-targets`（与 CI 一致），并补充单一前端架构、`FrontendAssets` 类型擦除约束、开发模式下网页端需先 `pnpm build` 等要点

## v0.3.4

- 安全：局域网 `GET /videos` 不再返回本机绝对路径（新增 `VideoSummary`），避免泄露服务器文件系统结构
- 新增 `get_share_status` 命令：webview 重载后恢复共享状态与文件列表，修复"服务器仍在运行却无法停止"的死角
- `/refresh-status` 新增 `pending` 字段，前端不再靠 `message` 文案判断刷新结果（改文案会静默改变轮询行为）
- 修复视频列表搜索过滤瞬间可能渲染越界项并抛错（虚拟列表 count 滞后一帧同步）
- 修复 `Host` 头解析对 IPv6 字面量（`[::1]:6008`）误判为非法而返回 403
- 修复日志全局级别写死 Info 导致 `RUST_LOG=debug` 完全失效
- 修复共享端口自增可能发生 u16 溢出（改用 saturating_add）
- 修复启停服务器加锁顺序不一致（threads/state 顺序相反）可能导致的 ABBA 死锁
- 视频缓存改为"临时文件 + 重命名"原子写入，加载失败日志从 debug 提升为 warn（不再静默退化）
- 移除 `panic = "abort"` 构建下永远无法生效的 `catch_unwind`；清理 IP 限流中的死分支
- 桌面端切换文件夹时先清空列表，避免扫描失败后残留上一个文件夹的内容
- 新增 GitHub Actions CI（fmt / clippy / test / svelte-check / build，Windows runner）
- 新增 `pnpm-workspace.yaml`：pnpm 11 起设置项不再从 `package.json#pnpm` 读取，且需显式放行 esbuild 构建脚本

## v0.3.3

- 修复扫描缓存陈旧问题：缓存改为逐文件校验（路径/大小/修改时间），子目录增删文件也能感知
- 修复 ETag 碰撞问题：改为全量指纹并缓存，避免网页端列表长期陈旧
- Range 请求边界修复：支持 `bytes=-N` 后缀范围，非法/越界请求返回 416
- 停止服务器改为并行等待 worker 退出，最多 5 秒
- 密码 pepper 改为首次启动随机生成（旧配置自动迁移），移除硬编码常量
- asset 协议访问范围运行时动态放行，移除全盘 `**` 范围
- 共享端口被占用时自动尝试下一个端口
- 新增文件日志（应用数据目录，5MB 轮转）
- 密码状态迁入 AppState 统一管理；视频流并发上限 16

## v0.3.2

- 优化软件与网页端界面，统一深色主题
- 增加局域网访问密码保护（4 位 PIN + Argon2id + IP 限流）
- 增加二维码分享
- 扫描结果缓存，重复扫描秒开

## v0.3.1

- 实现单数据源架构，软件端和网页端数据同步
- 修复网页端视频播放问题
