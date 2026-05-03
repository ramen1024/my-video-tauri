use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::error::AppError;
use crate::models::VideoFile;
use crate::utils::{format_system_time, is_root_directory};
use crate::{CANCEL_SCAN_FLAG, SHARED_VIDEOS, SHARED_FOLDER_PATH};

use std::sync::Arc;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
];

#[tauri::command]
pub async fn scan_videos(folder_path: String) -> Result<Arc<Vec<VideoFile>>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        scan_videos_sync(folder_path)?;
        Ok(SHARED_VIDEOS.read().clone())
    })
    .await
    .map_err(|e| AppError::Other(format!("扫描任务执行失败: {}", e)))?
}

pub(crate) fn scan_videos_sync(folder_path: String) -> Result<(), AppError> {
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

    CANCEL_SCAN_FLAG.store(false, Ordering::Relaxed);

    let entries: Vec<_> = WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            if !e.file_type().is_file() {
                return false;
            }
            let ext = e.path().extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());
            matches!(ext, Some(ref e) if VIDEO_EXTENSIONS.contains(&e.as_str()))
        })
        .collect();

    if CANCEL_SCAN_FLAG.load(Ordering::Relaxed) {
        return Err(AppError::ScanCancelled);
    }

    let mut videos: Vec<VideoFile> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if CANCEL_SCAN_FLAG.load(Ordering::Relaxed) {
                return None;
            }

            let path = entry.path();
            let ext_lower = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())?;

            let metadata = entry.metadata().ok().or_else(|| fs::metadata(path).ok())?;
            let size = metadata.len();

            if size < 1_048_576 {
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

    if CANCEL_SCAN_FLAG.load(Ordering::Relaxed) {
        return Err(AppError::ScanCancelled);
    }

    videos.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
    });

    let mut shared = SHARED_VIDEOS.write();
    *shared = Arc::new(videos);

    let mut shared_path = SHARED_FOLDER_PATH.write();
    *shared_path = folder_path;

    Ok(())
}

#[tauri::command]
pub fn get_shared_videos() -> Result<Arc<Vec<VideoFile>>, AppError> {
    Ok(SHARED_VIDEOS.read().clone())
}

#[tauri::command]
pub fn cancel_scan() {
    CANCEL_SCAN_FLAG.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn play_video(file_path: String) -> Result<(), AppError> {
    let path = Path::new(&file_path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ref e) if VIDEO_EXTENSIONS.contains(&e.as_str()) => {}
        _ => return Err(AppError::InvalidPath("不允许打开非视频文件".to_string())),
    }

    let shared_folder = SHARED_FOLDER_PATH.read().clone();
    if !shared_folder.is_empty() {
        if let Ok(canonical_path) = path.canonicalize() {
            if let Ok(canonical_base) = Path::new(&shared_folder).canonicalize() {
                if !canonical_path.starts_with(&canonical_base) {
                    return Err(AppError::InvalidPath("只能打开共享文件夹内的视频文件".to_string()));
                }
            }
        }
    }

    tauri_plugin_opener::open_path(&file_path, None::<&str>)
        .map_err(|e| AppError::IoError(format!("无法打开视频: {}", e)))
}
