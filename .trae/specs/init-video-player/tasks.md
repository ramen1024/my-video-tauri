# Tasks - 视频扫描播放工具

## 项目初始化

- [x] Task 1: 初始化 Tauri + Svelte 项目
  - [x] SubTask 1.1: 使用 pnpm create tauri-app 创建项目骨架
  - [x] SubTask 1.2: 选择 Svelte 作为前端框架
  - [x] SubTask 1.3: 验证项目能够正常运行 `pnpm tauri dev`

- [x] Task 2: 配置项目环境
  - [x] SubTask 2.1: 添加 pnpm 依赖
  - [x] SubTask 2.2: 安装 Tauri CLI
  - [x] SubTask 2.3: 配置 Cargo 路径到系统 Path

## 核心功能开发

- [x] Task 3: 实现文件夹选择功能
  - [x] SubTask 3.1: 创建"选择文件夹"按钮组件
  - [x] SubTask 3.2: 调用 Tauri dialog API 打开文件夹选择对话框
  - [x] SubTask 3.3: 获取用户选择的文件夹路径

- [x] Task 4: 实现视频文件扫描功能（Rust 后端）
  - [x] SubTask 4.1: 创建 Tauri 命令 `scan_videos`
  - [x] SubTask 4.2: 使用 walkdir 库递归遍历目录
  - [x] SubTask 4.3: 实现视频文件格式过滤（mp4, mkv, avi, mov, wmv, flv, webm, m4v, mpg, mpeg）
  - [x] SubTask 4.4: 获取文件元信息（文件名、路径、大小、创建日期）
  - [x] SubTask 4.5: 返回 JSON 格式的视频列表

- [x] Task 5: 实现视频列表展示界面
  - [x] SubTask 5.1: 创建 VideoList 组件
  - [x] SubTask 5.2: 实现列表项样式（Win11 风格）
  - [x] SubTask 5.3: 实现空列表提示
  - [x] SubTask 5.4: 实现列表滚动和 hover 效果

- [x] Task 6: 实现视频播放功能
  - [x] SubTask 6.1: 点击列表项触发播放
  - [x] SubTask 6.2: 使用 Tauri shell API 调用系统默认播放器
  - [x] SubTask 6.3: 实现播放错误处理和提示

- [x] Task 7: 实现刷新功能
  - [x] SubTask 7.1: 创建"刷新"按钮
  - [x] SubTask 7.2: 点击刷新时重新扫描同一文件夹

## UI/UX 完善

- [x] Task 8: Win11 风格样式美化
  - [x] SubTask 8.1: 应用 Win11 设计规范（颜色、圆角、阴影）
  - [x] SubTask 8.2: 优化按钮和列表项样式
  - [x] SubTask 8.3: 添加加载状态提示

- [x] Task 9: 添加标题栏和控制按钮
  - [x] SubTask 9.1: 配置 Tauri 窗口为标准窗口（带标题栏）
  - [x] SubTask 9.2: 确保最小化、最大化、关闭按钮正常工作

## 打包发布

- [x] Task 10: 构建便携版
  - [x] SubTask 10.1: 运行 `pnpm tauri build` 编译项目
  - [x] SubTask 10.2: 验证生成的可执行文件
  - [ ] SubTask 10.3: 测试便携版功能

---

# Task Dependencies
- Task 3 依赖 Task 1（项目初始化完成）
- Task 4 依赖 Task 1（项目初始化完成）
- Task 5 依赖 Task 3、Task 4（需要文件夹选择和扫描功能）
- Task 6 依赖 Task 5（需要列表展示）
- Task 7 依赖 Task 3、Task 4（复用扫描逻辑）
- Task 8 依赖 Task 5（基础界面完成）
- Task 9 依赖 Task 1（窗口配置）
- Task 10 依赖所有功能开发任务完成
