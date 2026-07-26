//! 视频扫描与管理命令
//!
//! 提供视频文件的扫描、获取、播放和取消扫描等 Tauri IPC 命令。
//! 扫描使用 rayon 并行处理，支持通过 AppState 中的取消标志中止扫描。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use tauri::State;
use walkdir::WalkDir;

use crate::constants::{MIN_VIDEO_FILE_SIZE_BYTES, VIDEO_SUPPORTED_EXTENSIONS};
use crate::error::AppError;
use crate::models::VideoFile;
use crate::utils::{format_system_time, is_root_directory};
use crate::AppState;

/// 扫描指定文件夹中的视频文件
///
/// 在阻塞线程中执行扫描，完成后返回共享的视频列表。
/// 扫描结果写入 AppState 中的 shared_videos 和 shared_folder_path。
#[tauri::command]
pub async fn scan_videos(
    folder_path: String,
    state: State<'_, AppState>,
) -> Result<Arc<Vec<VideoFile>>, AppError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        scan_videos_sync(folder_path, &app_state)?;
        Ok(app_state.shared_videos())
    })
    .await
    .map_err(|e| AppError::Other(format!("扫描任务执行失败: {}", e)))?
}

/// 同步执行视频扫描（在 spawn_blocking 中调用）
///
/// 流程：验证路径 → 遍历文件 → 并行提取元数据 → 按名称排序 → 写入 AppState
pub(crate) fn scan_videos_sync(
    folder_path: String,
    app_state: &AppState,
) -> Result<(), AppError> {
    let path = Path::new(&folder_path);

    if !path.exists() {
        return Err(AppError::InvalidPath("文件夹不存在".to_string()));
    }
    if !path.is_dir() {
        return Err(AppError::InvalidPath("路径不是文件夹".to_string()));
    }

    if is_root_directory(path) {
        return Err(AppError::InvalidPath(
            "扫描磁盘根目录可能会花费大量时间并导致程序卡住，请选择一个具体的文件夹".to_string(),
        ));
    }

    let base_path = Path::new(&folder_path).to_path_buf();

    app_state.reset_cancel_scan_flag();

    let entries: Vec<_> = WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            if !e.file_type().is_file() {
                return false;
            }
            let ext = e
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());
            matches!(ext, Some(ref e) if VIDEO_SUPPORTED_EXTENSIONS.contains(&e.as_str()))
        })
        .collect();

    if app_state.is_scan_cancelled() {
        return Err(AppError::ScanCancelled);
    }

    let mut videos: Vec<VideoFile> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if app_state.is_scan_cancelled() {
                return None;
            }

            let path = entry.path();
            let ext_lower = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())?;

            let metadata = entry.metadata().ok().or_else(|| fs::metadata(path).ok())?;
            let size = metadata.len();

            if size < MIN_VIDEO_FILE_SIZE_BYTES {
                return None;
            }

            let modified = metadata.modified().ok().map(format_system_time);

            let relative_path = path
                .strip_prefix(&base_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            Some(VideoFile {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                relative_path,
                size,
                modified,
                extension: ext_lower,
            })
        })
        .collect();

    if app_state.is_scan_cancelled() {
        return Err(AppError::ScanCancelled);
    }

    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    app_state.set_shared_videos(videos);
    app_state.set_shared_folder_path(folder_path);

    Ok(())
}

/// 获取当前共享的视频列表
#[tauri::command]
pub fn get_shared_videos(state: State<'_, AppState>) -> Result<Arc<Vec<VideoFile>>, AppError> {
    Ok(state.shared_videos())
}

/// 取消正在进行的视频扫描
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.set_cancel_scan_flag();
}

/// 使用系统默认播放器打开视频文件
///
/// 安全检查：仅允许打开视频扩展名文件，且必须在共享文件夹路径内
#[tauri::command]
pub fn play_video(file_path: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let path = Path::new(&file_path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ref e) if VIDEO_SUPPORTED_EXTENSIONS.contains(&e.as_str()) => {}
        _ => return Err(AppError::InvalidPath("不允许打开非视频文件".to_string())),
    }

    let shared_folder = state.shared_folder_path();
    if !shared_folder.is_empty() {
        let canonical_path = path
            .canonicalize()
            .map_err(|_| AppError::InvalidPath("无法解析视频文件路径".to_string()))?;
        let canonical_base = Path::new(&shared_folder)
            .canonicalize()
            .map_err(|_| AppError::InvalidPath("无法解析共享文件夹路径".to_string()))?;
        if !canonical_path.starts_with(&canonical_base) {
            return Err(AppError::InvalidPath(
                "只能打开共享文件夹内的视频文件".to_string(),
            ));
        }
    }

    tauri_plugin_opener::open_path(&file_path, None::<&str>)
        .map_err(|e| AppError::IoError(format!("无法打开视频: {}", e)))
}
