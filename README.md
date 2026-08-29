# 视频扫描器

一个基于 Tauri + SvelteKit 开发的跨平台视频扫描和局域网共享工具，个人自用，用于在家庭/办公室局域网中共享视频资源。

## 功能特性

### 核心功能
- **视频扫描**：快速扫描指定文件夹中的所有视频文件（支持 MP4、MKV、AVI、MOV 等格式）
- **扫描缓存**：扫描结果按文件夹持久化缓存，重复扫描秒开；子目录内增删改文件也能自动感知并使缓存失效
- **单数据源架构**：软件端和网页端共享同一数据源，确保数据一致性
- **局域网共享**：一键开启 HTTP 服务器，在同一网络下的任何设备（手机、平板、电视）上访问和播放视频
- **密码保护**：可选的 4 位数字访问密码，Argon2id 哈希存储，带 IP 登录频率限制（防暴力破解）
- **二维码分享**：自动生成访问地址二维码，手机扫码即可访问
- **列表浏览**：深色卡片式界面，虚拟滚动支持万级文件流畅浏览，可按文件名/大小/修改时间排序与搜索

### 技术亮点
- **视频流播放**：支持 HTTP Range 请求，可拖动进度条播放；流式响应独立线程写出，不阻塞其他请求
- **缓存优化**：网页端通过 ETag 增量刷新，列表未变化时零流量重渲染
- **稳健的 HTTP 服务**：Host 头校验防 DNS rebinding、路径穿越防护、响应安全头齐全
- **跨平台**：支持 Windows、macOS、Linux（日常开发与测试以 Windows 为主，其他平台未经充分验证）

## 使用指南

### 1. 扫描视频
1. 点击"选择文件夹"按钮
2. 选择包含视频文件的文件夹
3. 等待扫描完成，视频列表将自动显示

### 2. 播放视频
- **应用内播放**：浏览器原生支持的格式（MP4、WebM、M4V）在应用内置播放器中播放
- **系统播放器**：其他格式（MKV、AVI 等）点击"打开"使用系统默认播放器

### 3. 局域网共享
1. 点击"局域网共享"按钮
2. 查看访问地址或扫描二维码
3. 在同一网络下的其他设备浏览器中输入地址
4. 即可访问视频列表并在线播放（端口被占用时会自动尝试下一个端口）

网页端 HTTP 接口的完整说明见 [docs/api.md](docs/api.md)。

### 4. 刷新数据
- 点击"刷新"按钮可重新扫描文件夹
- 网页端每 30 秒自动刷新一次，列表未变化时不重新渲染

## 安全提示

- 局域网共享通过 **HTTP 明文传输**，建议仅在可信的家庭或办公局域网中使用，避免在公共网络中开启共享。
- 用户设置的密码使用 **Argon2id + 随机 salt + 随机 pepper** 进行哈希存储，不会以明文形式保存。
- 默认的 4 位数字密码本身熵较低，主要作用是防止随意访问，**不应作为高强度安全认证手段**。如需更高安全性，请设置更复杂的密码并在可信网络中使用。
- 网页端对视频文件路径进行严格校验，仅可访问共享文件夹内的文件。

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
| `video_cache.json` | 扫描结果缓存（按文件夹） | 扫描结果与缓存不一致时自动更新 |
| `video-scanner.log` | 运行日志，超 5MB 自动轮转为 `.1` | 启动时创建，运行期间持续追加 |

另外在 Windows 上，WebView2 浏览器内核自身的缓存、LocalStorage 等数据保存在 `%LOCALAPPDATA%\com.myvideo.scanner\`（框架行为，与本项目代码无关）。

清理说明：删除上述文件不影响共享文件夹本身；删除 `password_config.json` 后需重新设置访问密码，删除 `video_cache.json` 后下次扫描会全量重建缓存。

## 技术栈

- **前端**：SvelteKit + Vite + Svelte 5（runes）
- **后端**：Rust + Tauri 2
- **HTTP 服务器**：tiny_http（多 worker + 流式响应独立线程）
- **密码哈希**：Argon2id
- **虚拟滚动**：@tanstack/svelte-virtual

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

# 构建 release 版本（前端 + Rust + 安装包）
pnpm tauri build
```

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
