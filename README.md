# 视频扫描器

一个基于 Tauri + SvelteKit 开发的跨平台视频扫描和局域网共享工具，个人自用，用于在家庭/办公室局域网中共享视频资源。

## 功能特性

### 核心功能
- **视频扫描**：快速扫描指定文件夹中的所有视频文件（支持 MP4、MKV、AVI、MOV 等格式），每次扫描都重新遍历目录，文件增删改随时可见
- **单一前端**：桌面端与网页端**跑的是同一份 Svelte 代码**（同一构建产物），只是数据来源不同（桌面端走 IPC，网页端走 HTTP JSON 接口）——不存在两套界面实现需要同步维护
- **局域网共享**：一键开启 HTTP 服务器，在同一网络下的任何设备（手机、平板、电视）上访问和播放视频
- **密码保护**：可选的 4 位数字访问密码，Argon2id 哈希存储，带 IP 登录频率限制（防暴力破解）
- **二维码分享**：自动生成访问地址二维码，手机扫码即可访问
- **列表浏览**：深色卡片式界面，虚拟滚动支持万级文件流畅浏览，可按文件名/大小/修改时间排序与搜索

### 技术亮点
- **视频流播放**：支持 HTTP Range 请求，可拖动进度条播放；流式响应独立线程写出，不阻塞其他请求
- **缓存优化**：网页端按 ETag 增量刷新（`If-None-Match` → `304`），列表未变化时既不重传数据也不重渲染界面
- **稳健的 HTTP 服务**：Host 头校验防 DNS rebinding、路径穿越防护、视频端点扩展名白名单、CSP nonce 化的严格脚本策略、响应安全头齐全
- **跨平台**：支持 Windows、macOS、Linux（日常开发与测试以 Windows 为主，其他平台未经充分验证）

## 使用指南

### 1. 扫描视频
1. 点击"选择文件夹"按钮
2. 选择包含视频文件的文件夹
3. 等待扫描完成，视频列表将自动显示

### 2. 播放视频
- **应用内播放**：浏览器原生支持的格式（MP4、WebM、M4V）在应用内置播放器中播放
- **系统播放器**：其他格式（MKV、AVI 等）点击"打开"使用系统默认播放器
- **网页端**：统一由浏览器/设备自身的播放能力决定，无法解码时会在界面上给出提示

### 3. 局域网共享
1. 点击"局域网共享"按钮
2. 查看访问地址或扫描二维码
3. 在同一网络下的其他设备浏览器中输入地址
4. 即可访问视频列表并在线播放（端口被占用时会自动尝试下一个端口）

网页端访问到的界面与桌面端是**同一个前端**，只是自动隐藏了选择文件夹、共享开关、
访问密码等本机专属入口。网页端 HTTP 接口的完整说明见 [docs/api.md](docs/api.md)。

### 4. 刷新数据
- 点击"刷新"按钮可重新扫描文件夹
- 网页端每 30 秒自动刷新一次，列表未变化时不重新渲染

## 安全提示

- 局域网共享通过 **HTTP 明文传输**，建议仅在可信的家庭或办公局域网中使用，避免在公共网络中开启共享。
- 用户设置的密码使用 **Argon2id + 随机 salt + 随机 pepper** 进行哈希存储，不会以明文形式保存。
- 默认的 4 位数字密码本身熵较低，主要作用是防止随意访问，**不应作为高强度安全认证手段**。如需更高安全性，请设置更复杂的密码并在可信网络中使用。
- 网页端对视频文件路径进行严格校验：既要求文件位于共享文件夹内（防路径穿越），也要求扩展名在受支持的视频格式白名单内，**共享文件夹中的其他文件不会被提供下载**。
- 网页端只允许来自本机 IP / localhost 的 `Host` 头，避免恶意网站通过 DNS rebinding 读取共享列表。

## 数据存储说明

软件运行时**只在自己的数据目录写入文件**，共享的视频文件夹全程只读（扫描与播放均为只读打开，不写入、不重命名、不删除其中任何文件）。

应用数据目录（按操作系统）：

- Windows：`%APPDATA%\com.myvideo.scanner\`
- macOS：`~/Library/Application Support/com.myvideo.scanner/`
- Linux：`~/.local/share/com.myvideo.scanner/`

目录内的文件：

| 文件 | 用途 | 写入时机 |
|------|------|----------|
| `password_config.json` | 密码配置（Argon2id 哈希、启用状态、随机 pepper） | 首次启动生成 pepper；设置/清除/启用/停用密码时更新 |
| `video-scanner.log` | 运行日志，超 5MB 自动轮转为 `.1` | 启动时创建，运行期间持续追加 |

另外在 Windows 上，WebView2 浏览器内核自身的缓存、LocalStorage 等数据保存在 `%LOCALAPPDATA%\com.myvideo.scanner\`（框架行为，与本项目代码无关）。

清理说明：删除上述文件不影响共享文件夹本身；删除 `password_config.json` 后需重新设置访问密码。

> 旧版本（≤ v0.3.4）会在数据目录留下 `video_cache.json` 扫描结果缓存，v0.3.5 起已移除该缓存机制，此文件不再被读写，可以手动删除。

## 技术栈

- **前端**：SvelteKit + Vite + Svelte 5（runes）
- **后端**：Rust + Tauri 2
- **HTTP 服务器**：tiny_http（多 worker + 流式响应独立线程）
- **密码哈希**：Argon2id
- **虚拟滚动**：@tanstack/svelte-virtual

桌面端与网页端的差异集中在 `src/lib/platform/`：一份 `Platform` 接口，两个实现
（`desktop.ts` 走 Tauri IPC，`web.ts` 走 HTTP 接口），应用代码只依赖接口与
能力标志（能否选择文件夹、能否控制共享、是否需要轮询列表）。

## 开发环境与构建

目前未提供预编译安装包，暂需从源码构建。

### 环境要求

- [Rust](https://www.rust-lang.org/) 工具链，建议 **1.80 或更高版本**
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) 包管理器
- Rust crates 已配置 USTC 镜像（`src-tauri/.cargo/config.toml`）；境外网络环境可删除其中的镜像配置，改用 crates.io 官方源

### 常用命令

```powershell
# 仅启动前端开发服务器（Vite + SvelteKit）
pnpm dev

# 启动完整 Tauri 桌面应用开发模式
pnpm tauri dev

# 类型检查（前后端）
pnpm check
cargo check  # 在 src-tauri/ 目录下

# 运行 Rust 单元测试
cargo test   # 在 src-tauri/ 目录下

# 额外运行需要真实前端产物的端到端测试（需先执行 pnpm build）
cargo test -- --ignored   # 在 src-tauri/ 目录下

# 构建 release 版本（前端 + Rust + Windows NSIS 安装包）
pnpm tauri build
```

`pnpm build` 末尾会自动校验前端产物形状是否满足服务端（内嵌 HTTP 服务器）的假设，
不满足会直接报错而不是留到运行时白屏。

构建产物：

- 可执行文件：`src-tauri/target/release/视频扫描器.exe`
- 安装包：`src-tauri/target/release/bundle/nsis/视频扫描器_<版本>_x64-setup.exe`

`bundle.targets` 固定为 `nsis`（单文件、自包含的 Windows 安装包，无需额外工具链）。
早先配置里写的是 `["app"]`——那是 macOS 专用的 `.app` 包类型，在 Windows 上不会产出
任何安装包；改用框架默认目标集则会先尝试 MSI（WiX），在本机 `light.exe` 阶段失败并
中断整个打包，因此显式选择已验证可用的 `nsis`。在 macOS / Linux 上打包时请用
`pnpm tauri build --bundles app,dmg` 或 `--bundles deb,appimage` 覆盖。

### 网页端开发注意

内嵌 HTTP 服务器提供的是磁盘上的 `build/` 目录，而 `pnpm tauri dev` 下 **webview**
走的是 Vite dev server。因此修改前端后，桌面端界面会热更新，但**网页端**要生效需要
先执行一次 `pnpm build`。

### Windows 平台说明

- 构建 Windows 安装包需要安装 [Microsoft C++ 生成工具](https://visualstudio.microsoft.com/visual-cpp-build-tools/) 或 Visual Studio 的"使用 C++ 的桌面开发"工作负载。
- 项目通过 `if-addrs` crate 获取本机网络接口信息，无需调用外部子进程。
- Release 模式下日志同时写入应用数据目录下的 `video-scanner.log`（超过 5MB 自动轮转），便于排查问题。

## 更新日志

历史版本变更见 [CHANGELOG.md](CHANGELOG.md)。

## 致谢

- [Tauri](https://tauri.app/) - 跨平台桌面应用框架
- [SvelteKit](https://kit.svelte.dev/) - 前端应用框架
- [tiny_http](https://github.com/tiny-http/tiny-http) - Rust 轻量级 HTTP 服务器

## 许可证

本项目基于 [GPL-3.0](LICENSE) 许可证开源：任何人使用、修改、分发本软件及其衍生版本，都须以相同许可证开源源码。
