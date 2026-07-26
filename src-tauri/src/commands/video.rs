//! 视频扫描与管理命令
//!
//! 提供视频文件的扫描、获取、播放和取消扫描等 Tauri IPC 命令。
//! 扫描使用 rayon 并行处理，支持通过 AppState 中的取消标志中止扫描。

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use tauri::State;
use walkdir::WalkDir;

use crate::constants::{MIN_VIDEO_FILE_SIZE_BYTES, VIDEO_SUPPORTED_EXTENSIONS};
use crate::error::AppError;
use crate::models::VideoFile;
use crate::utils::{format_system_time, is_root_directory};
use crate::video_cache::VideoCacheEntry;
use crate::AppState;

/// 扫描指定文件夹中的视频文件
///
/// 在阻塞线程中执行扫描，完成后返回共享的视频列表。
/// 扫描结果写入 AppState 中的 shared_videos 和 shared_folder_path。
#[tauri::command]
pub async fn scan_videos(
    folder_path: String,
    use_cache: bool,
    state: State<'_, AppState>,
) -> Result<Arc<Vec<VideoFile>>, AppError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        scan_videos_sync(folder_path, &app_state, use_cache)?;
        Ok(app_state.shared_videos())
    })
    .await
    .map_err(|e| AppError::Other(format!("扫描任务执行失败: {}", e)))?
}

/// 同步执行视频扫描（在 spawn_blocking 中调用）
///
/// 流程：验证路径 → 检查缓存 → 遍历文件 → 并行提取元数据 → 按名称排序 → 写入 AppState
///
/// # 参数
/// - `folder_path`: 要扫描的文件夹路径
/// - `app_state`: 应用状态
/// - `use_cache`: 是否允许使用缓存。正常扫描为 `true`；手动刷新应为 `false`，
///   但扫描完成后仍会将结果写回缓存。
pub(crate) fn scan_videos_sync(
    folder_path: String,
    app_state: &AppState,
    use_cache: bool,
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

    let folder_modified_time = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()))
        .unwrap_or(0);

    if use_cache {
        let cached_entry = app_state
            .video_cache()
            .lock()
            .get(&folder_path, folder_modified_time)
            .cloned();

        if let Some(entry) = cached_entry {
            let count = entry.videos.len();
            let folder = folder_path.clone();
            app_state.set_shared_videos(entry.videos);
            app_state.set_shared_folder_path(folder_path);
            log::info!("[扫描] 从缓存加载 {} 个视频: {}", count, folder);
            return Ok(());
        }
    }

    let base_path = Path::new(&folder_path).to_path_buf();

    app_state.reset_cancel_scan_flag();

    let entries: Vec<_> = WalkDir::new(&folder_path)
        .follow_links(false)
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

    let cached_at = chrono::Utc::now().timestamp();
    let cache_entry = VideoCacheEntry {
        videos: videos.clone(),
        folder_modified_time,
        cached_at,
    };

    if let Err(e) = app_state
        .video_cache()
        .lock()
        .set(folder_path.clone(), cache_entry)
    {
        log::warn!("[扫描] 保存视频缓存失败: {}", e);
    }

    let video_count = videos.len();
    app_state.set_shared_videos(videos);
    app_state.set_shared_folder_path(folder_path);

    log::info!("[扫描] 完成全量扫描，共 {} 个视频", video_count);

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

/// 校验播放请求是否合法
///
/// 仅允许打开视频扩展名文件；当已设置共享目录时，还要求文件必须位于共享目录内。
fn validate_play_video(file_path: &str, shared_folder: &str) -> Result<(), AppError> {
    if shared_folder.is_empty() {
        return Err(AppError::InvalidPath("未设置共享目录".to_string()));
    }

    let path = Path::new(file_path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ref e) if VIDEO_SUPPORTED_EXTENSIONS.contains(&e.as_str()) => {}
        _ => return Err(AppError::InvalidPath("不允许打开非视频文件".to_string())),
    }

    let canonical_path = path
        .canonicalize()
        .map_err(|_| AppError::InvalidPath("无法解析视频文件路径".to_string()))?;
    let canonical_base = Path::new(shared_folder)
        .canonicalize()
        .map_err(|_| AppError::InvalidPath("无法解析共享文件夹路径".to_string()))?;
    if !canonical_path.starts_with(&canonical_base) {
        return Err(AppError::InvalidPath(
            "只能打开共享文件夹内的视频文件".to_string(),
        ));
    }

    Ok(())
}

/// 使用系统默认播放器打开视频文件
///
/// 安全检查：仅允许打开视频扩展名文件，且必须在共享文件夹路径内
#[tauri::command]
pub fn play_video(file_path: String, state: State<'_, AppState>) -> Result<(), AppError> {
    validate_play_video(&file_path, &state.shared_folder_path())?;

    tauri_plugin_opener::open_path(&file_path, None::<&str>)
        .map_err(|e| AppError::IoError(format!("无法打开视频: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let mut name = prefix.to_string();
        name.push_str("_");
        name.push_str(&std::process::id().to_string());
        name.push_str("_");
        name.push_str(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_string(),
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    fn create_test_video(path: &Path, size: u64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("创建父目录失败");
        }
        let mut file = fs::File::create(path).expect("创建测试视频文件失败");
        let buf = vec![0u8; 8192];
        let mut remaining = size;
        while remaining > 0 {
            let chunk = std::cmp::min(remaining, buf.len() as u64) as usize;
            file.write_all(&buf[..chunk]).expect("写入测试视频数据失败");
            remaining -= chunk as u64;
        }
    }

    #[test]
    fn test_validate_play_video_rejects_empty_shared_folder() {
        let result = validate_play_video("C:\\video.mp4", "");
        assert!(
            matches!(result, Err(AppError::InvalidPath(ref msg)) if msg == "未设置共享目录"),
            "未设置共享目录时应直接拒绝"
        );
    }

    #[test]
    fn test_validate_play_video_rejects_non_video() {
        let shared = make_temp_dir("play_non_video");
        let file = shared.join("doc.txt");
        fs::File::create(&file).expect("创建文本文件失败");

        let result = validate_play_video(file.to_str().unwrap(), shared.to_str().unwrap());
        assert!(
            matches!(result, Err(AppError::InvalidPath(ref msg)) if msg == "不允许打开非视频文件"),
            "非视频文件应被拒绝"
        );
        let _ = fs::remove_dir_all(&shared);
    }

    #[test]
    fn test_validate_play_video_accepts_video_in_shared_folder() {
        let shared = make_temp_dir("play_valid");
        let video = shared.join("movie.mp4");
        create_test_video(&video, MIN_VIDEO_FILE_SIZE_BYTES);

        let result = validate_play_video(video.to_str().unwrap(), shared.to_str().unwrap());
        assert!(result.is_ok(), "共享目录内的视频文件应通过校验");
        let _ = fs::remove_dir_all(&shared);
    }

    #[test]
    fn test_validate_play_video_rejects_video_outside_shared_folder() {
        let shared = make_temp_dir("play_shared");
        let outside = make_temp_dir("play_outside");
        let video = outside.join("outside.mp4");
        create_test_video(&video, MIN_VIDEO_FILE_SIZE_BYTES);

        let result = validate_play_video(video.to_str().unwrap(), shared.to_str().unwrap());
        assert!(
            matches!(result, Err(AppError::InvalidPath(ref msg)) if msg == "只能打开共享文件夹内的视频文件"),
            "共享目录外的视频文件应被拒绝"
        );
        let _ = fs::remove_dir_all(&shared);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn test_scan_videos_ignores_symlinks() {
        let scan_dir = make_temp_dir("scan_symlink");
        let target_dir = make_temp_dir("scan_symlink_target");

        let real_video = scan_dir.join("real.mp4");
        create_test_video(&real_video, MIN_VIDEO_FILE_SIZE_BYTES);

        let outside_video = target_dir.join("outside.mp4");
        create_test_video(&outside_video, MIN_VIDEO_FILE_SIZE_BYTES);

        let symlink_video = scan_dir.join("link.mp4");
        #[cfg(windows)]
        let symlink_created = std::os::windows::fs::symlink_file(&outside_video, &symlink_video);
        #[cfg(unix)]
        let symlink_created = std::os::unix::fs::symlink(&outside_video, &symlink_video);

        if let Err(e) = symlink_created {
            eprintln!("当前环境无法创建符号链接，跳过测试: {}", e);
            let _ = fs::remove_dir_all(&scan_dir);
            let _ = fs::remove_dir_all(&target_dir);
            return;
        }

        let app_state = AppState::new();
        scan_videos_sync(scan_dir.to_string_lossy().to_string(), &app_state, false)
            .expect("扫描应成功完成");

        let videos = app_state.shared_videos();
        assert_eq!(videos.len(), 1, "应只包含真实视频文件");
        assert_eq!(videos[0].name, "real.mp4", "应忽略符号链接指向的文件");

        let _ = fs::remove_dir_all(&scan_dir);
        let _ = fs::remove_dir_all(&target_dir);
    }
}
