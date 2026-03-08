# 视频扫描播放工具规格文档

## Why
用户需要一个 Windows 11 风格的便携版桌面应用程序，能够扫描指定文件夹下的视频文件，以列表形式展示视频信息，并支持点击播放。

## What Changes
- 创建基于 Svelte + Tauri 的 Windows 桌面应用程序
- 实现视频文件扫描功能，支持常见视频格式（mp4, mkv, avi, mov, wmv, flv, webm）
- 实现视频列表展示界面，采用 Win11 风格设计
- 实现视频播放功能
- 便携版打包，不依赖安装

## Impact
- Affected specs: 视频扫描、列表展示、视频播放
- Affected code: 前端 Svelte 组件、Tauri Rust 后端命令

---

## ADDED Requirements

### Requirement: 视频文件扫描功能
系统 SHALL 能够递归扫描用户指定的文件夹，查找所有支持的视频文件。

#### Scenario: 扫描指定文件夹
- **GIVEN** 用户已指定要扫描的文件夹路径
- **WHEN** 用户触发扫描操作
- **THEN** 系统应扫描该文件夹及其子文件夹中的所有视频文件
- **AND** 返回文件列表，包含文件名、路径、文件大小、时长（若可获取）、创建日期

#### Scenario: 支持的视频格式
- **WHEN** 扫描过程中遇到以下格式的文件
- **THEN** 系统 SHALL 识别为视频文件：mp4, mkv, avi, mov, wmv, flv, webm, m4v, mpg, mpeg

---

### Requirement: 视频列表展示功能
系统 SHALL 以列表形式展示扫描到的视频文件信息，采用 Windows 11 设计风格。

#### Scenario: 视频列表显示
- **GIVEN** 已完成视频扫描
- **WHEN** 系统显示视频列表
- **THEN** 每个列表项应显示：视频文件名、文件大小、文件路径
- **AND** 列表应支持滚动浏览
- **AND** 列表项 hover 时应有视觉反馈

#### Scenario: 空列表提示
- **GIVEN** 扫描结果为空
- **WHEN** 显示列表
- **THEN** 应显示提示信息"未找到视频文件"

---

### Requirement: 视频播放功能
系统 SHALL 支持点击列表中的视频进行播放。

#### Scenario: 点击播放视频
- **GIVEN** 视频列表已显示
- **WHEN** 用户点击列表中的某个视频项
- **THEN** 系统应调用系统默认播放器打开该视频文件
- **OR** 系统内嵌播放器播放该视频

#### Scenario: 播放失败处理
- **GIVEN** 视频文件无法播放
- **WHEN** 播放操作执行失败
- **THEN** 应显示错误提示"无法播放该视频文件"

---

### Requirement: 文件夹选择功能
系统 SHALL 提供用户选择扫描文件夹的界面。

#### Scenario: 选择扫描文件夹
- **GIVEN** 用户在主界面
- **WHEN** 用户点击"选择文件夹"按钮
- **THEN** 应弹出系统文件夹选择对话框
- **AND** 用户选择文件夹后，系统自动开始扫描

---

### Requirement: 刷新功能
系统 SHALL 支持手动刷新视频列表。

#### Scenario: 刷新视频列表
- **GIVEN** 用户已扫描过文件夹
- **WHEN** 用户点击"刷新"按钮
- **THEN** 系统应重新扫描之前选择的文件夹
- **AND** 更新视频列表显示

---

### Requirement: 便携版打包
系统 SHALL 以便携版形式打包，无需安装即可运行。

#### Scenario: 便携版特性
- **WHEN** 用户获取打包后的程序
- **THEN** 程序应是一个独立的可执行文件（或包含 exe 的文件夹）
- **AND** 不应在系统目录创建文件
- **AND** 配置信息应保存在程序同目录下

---

## MODIFIED Requirements
无

## REMOVED Requirements
无

---

## 技术规格

### 技术栈
- **前端框架**: Svelte 5
- **桌面框架**: Tauri 2.x
- **包管理**: pnpm
- **目标平台**: Windows 11 (64位)
- **打包方式**: 便携版 (portable)

### UI 设计规范
- **设计风格**: Windows 11 Modern UI
- **配色方案**: 
  - 主色: #0078D4 (Windows Blue)
  - 背景: #F3F3F3 (浅色模式) / #202020 (深色模式)
  - 文字: #1A1A1A (浅色) / #FFFFFF (深色)
- **圆角**: 8px
- **阴影**: 0 2px 4px rgba(0,0,0,0.1)
- **字体**: Segoe UI Variable, Segoe UI, sans-serif

### 功能模块
1. **文件夹选择器** - Tauri dialog API
2. **视频扫描器** - Rust 后端实现，使用 walkdir 遍历目录
3. **视频列表组件** - Svelte 前端渲染
4. **播放器** - 内嵌 HTML5 video 或调用系统默认播放器
