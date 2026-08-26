//! 视频扫描与管理命令
//!
//! 提供视频文件的扫描、获取、播放和取消扫描等 Tauri IPC 命令。
//! 扫描使用单次目录遍历提取元数据，结果同时用于缓存校验与最终列表构建，
//! 缓存通过逐文件比对（路径/大小/修改时间）判定是否有效，支持通过
//! AppState 中的取消标志中止扫描。

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use tauri::State;
use walkdir::WalkDir;

use crate::constants::{MIN_VIDEO_FILE_SIZE_BYTES, VIDEO_SUPPORTED_EXTENSIONS};
use crate::error::AppError;
use crate::models::VideoFile;
use crate::utils::{allow_shared_folder_asset_scope, format_system_time, is_root_directory};
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
    app: tauri::AppHandle,
) -> Result<Arc<Vec<VideoFile>>, AppError> {
    let app_state = state.inner().clone();
    let scan_folder = folder_path.clone();

    let videos =
        tauri::async_runtime::spawn_blocking(move || -> Result<Arc<Vec<VideoFile>>, AppError> {
            scan_videos_sync(scan_folder, &app_state, use_cache)?;
            Ok(app_state.shared_videos())
        })
        .await
        .map_err(|e| AppError::Other(format!("扫描任务执行失败: {}", e)))??;

    // 桌面端通过 asset 协议播放视频，扫描成功后放行该文件夹
    allow_shared_folder_asset_scope(&app, &folder_path);

    Ok(videos)
}

/// RAII 守卫：持有扫描互斥标记，作用域结束时自动释放
///
/// 保证桌面扫描与网页刷新互斥，所有返回路径（缓存命中、错误、取消、成功）都会释放标记。
struct ScanGuard<'a>(&'a AppState);

impl<'a> ScanGuard<'a> {
    fn acquire(state: &'a AppState) -> Result<Self, AppError> {
        if !state.start_scan() {
            return Err(AppError::Other("扫描正在进行中，请稍后".to_string()));
        }
        Ok(Self(state))
    }
}

impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.0.finish_scan();
    }
}

/// 扫描过程中收集的原始文件信息（元数据已提取，可同时用于缓存校验与结果构建）
struct ScannedFile {
    /// 文件名（不含路径）
    name: String,
    /// 完整绝对路径
    path: String,
    /// 相对于扫描目录的相对路径（用于网页端访问）
    relative_path: String,
    /// 文件大小（字节）
    size: u64,
    /// 修改时间格式化字符串
    modified: Option<String>,
    /// 文件扩展名（小写）
    extension: String,
}

impl ScannedFile {
    fn into_video_file(self) -> VideoFile {
        VideoFile {
            name: self.name,
            path: self.path,
            relative_path: self.relative_path,
            size: self.size,
            modified: self.modified,
            extension: self.extension,
        }
    }
}

/// 与 [`ScannedFile::into_video_file`] 互逆的字段映射，供缓存校验测试构造样例
impl From<VideoFile> for ScannedFile {
    fn from(v: VideoFile) -> Self {
        Self {
            name: v.name,
            path: v.path,
            relative_path: v.relative_path,
            size: v.size,
            modified: v.modified,
            extension: v.extension,
        }
    }
}

/// 将遍历条目转换为 ScannedFile，应用扩展名与最小大小过滤
fn entry_to_scanned_file(entry: &walkdir::DirEntry, base_path: &Path) -> Option<ScannedFile> {
    if !entry.file_type().is_file() {
        return None;
    }

    let ext = entry
        .path()
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    match ext.as_deref() {
        Some(e) if VIDEO_SUPPORTED_EXTENSIONS.contains(&e) => {}
        _ => return None,
    }

    let metadata = entry
        .metadata()
        .ok()
        .or_else(|| fs::metadata(entry.path()).ok())?;
    let size = metadata.len();

    if size < MIN_VIDEO_FILE_SIZE_BYTES {
        return None;
    }

    Some(ScannedFile {
        name: entry
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: entry.path().to_string_lossy().to_string(),
        relative_path: entry
            .path()
            .strip_prefix(base_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        size,
        modified: metadata.modified().ok().map(format_system_time),
        extension: ext.unwrap_or_default(),
    })
}

/// 校验缓存视频列表与本次扫描结果是否完全一致（相对路径、大小、修改时间逐项比对）
///
/// 目录内任何新增、删除、改名或内容变化（含子目录中的变更）都会导致校验失败，
/// 从而触发重新扫描；校验通过即代表扫描结果与缓存等价，可直接复用缓存。
fn cache_matches(cached: &[VideoFile], scanned: &[ScannedFile]) -> bool {
    if cached.len() != scanned.len() {
        return false;
    }
    let mut cached_map: HashMap<&str, (u64, Option<&str>)> = HashMap::with_capacity(cached.len());
    for v in cached {
        cached_map.insert(v.relative_path.as_str(), (v.size, v.modified.as_deref()));
    }
    scanned.iter().all(|s| {
        cached_map
            .get(s.relative_path.as_str())
            .is_some_and(|(size, modified)| *size == s.size && *modified == s.modified.as_deref())
    })
}

/// 同步执行视频扫描（在 spawn_blocking 中调用）
///
/// 流程：验证路径 → 单次遍历提取元数据 → 校验缓存 → 按名称排序 → 写入 AppState。
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
    // 扫描互斥：桌面扫描与网页刷新不可并发执行，保证 shared_videos 写入互斥
    let _guard = ScanGuard::acquire(app_state)?;

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

    let base_path = path.to_path_buf();

    app_state.reset_cancel_scan_flag();

    // 单次遍历：过滤 + 提取元数据，结果同时用于缓存校验与最终列表构建
    let scanned: Vec<ScannedFile> = WalkDir::new(&folder_path)
        .follow_links(false)
        .into_iter()
        .take_while(|_| !app_state.is_scan_cancelled())
        .filter_map(|e| e.ok())
        .filter_map(|entry| entry_to_scanned_file(&entry, &base_path))
        .collect();

    if app_state.is_scan_cancelled() {
        return Err(AppError::ScanCancelled("扫描已取消".to_string()));
    }

    // 缓存校验：与扫描结果逐文件比对，目录内任何变化（含子目录）都会使缓存失效
    if use_cache {
        let cached_entry = app_state.video_cache().lock().get(&folder_path).cloned();
        if let Some(entry) = cached_entry {
            if cache_matches(&entry.videos, &scanned) {
                let count = entry.videos.len();
                app_state.set_shared_videos(entry.videos);
                app_state.set_shared_folder_path(folder_path.clone());
                log::info!("[扫描] 缓存有效，加载 {} 个视频: {}", count, folder_path);
                return Ok(());
            }
        }
    }

    let mut videos: Vec<VideoFile> = scanned
        .into_iter()
        .map(ScannedFile::into_video_file)
        .collect();

    videos.sort_by_cached_key(|v| v.name.to_lowercase());

    let cache_entry = VideoCacheEntry {
        videos: videos.clone(),
        cached_at: chrono::Utc::now().timestamp(),
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
    use crate::test_utils::{create_test_video, make_temp_dir, sample_video};
    use crate::AppState;
    use std::fs;

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

    fn sample_scanned(relative_path: &str, size: u64, modified: Option<&str>) -> ScannedFile {
        ScannedFile::from(sample_video(relative_path, size, modified))
    }

    #[test]
    fn test_cache_matches_identical_lists() {
        let cached = vec![
            sample_video("a.mp4", 100, Some("2024-01-01 10:00:00")),
            sample_video("b.mp4", 200, None),
        ];
        let scanned = vec![
            sample_scanned("a.mp4", 100, Some("2024-01-01 10:00:00")),
            sample_scanned("b.mp4", 200, None),
        ];
        assert!(cache_matches(&cached, &scanned), "完全一致的列表应校验通过");
    }

    #[test]
    fn test_cache_matches_rejects_changes() {
        let cached = vec![
            sample_video("a.mp4", 100, Some("2024-01-01 10:00:00")),
            sample_video("b.mp4", 200, None),
        ];

        // 大小变化
        let size_changed = vec![
            sample_scanned("a.mp4", 101, Some("2024-01-01 10:00:00")),
            sample_scanned("b.mp4", 200, None),
        ];
        assert!(!cache_matches(&cached, &size_changed), "大小变化应校验失败");

        // 改名（相对路径变化）
        let renamed = vec![
            sample_scanned("a.mp4", 100, Some("2024-01-01 10:00:00")),
            sample_scanned("bb.mp4", 200, None),
        ];
        assert!(!cache_matches(&cached, &renamed), "改名应校验失败");

        // 修改时间变化
        let time_changed = vec![
            sample_scanned("a.mp4", 100, Some("2024-06-01 10:00:00")),
            sample_scanned("b.mp4", 200, None),
        ];
        assert!(
            !cache_matches(&cached, &time_changed),
            "修改时间变化应校验失败"
        );

        // 多一个文件
        let extra = vec![
            sample_scanned("a.mp4", 100, Some("2024-01-01 10:00:00")),
            sample_scanned("b.mp4", 200, None),
            sample_scanned("c.mp4", 300, None),
        ];
        assert!(!cache_matches(&cached, &extra), "文件数量变化应校验失败");
    }

    #[test]
    fn test_cache_invalidated_by_subdirectory_change() {
        // 回归测试：子目录中新增文件不改变顶层目录 mtime，
        // 缓存校验必须感知到该变化并触发重新扫描
        let scan_dir = make_temp_dir("cache_subdir");
        let sub = scan_dir.join("sub");
        fs::create_dir_all(&sub).expect("创建子目录失败");
        create_test_video(&sub.join("one.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);

        let app_state = AppState::new();
        let path = scan_dir.to_string_lossy().to_string();

        scan_videos_sync(path.clone(), &app_state, true).expect("首次扫描应成功");
        assert_eq!(
            app_state.shared_videos().len(),
            1,
            "首次扫描应发现 1 个视频"
        );

        // 在子目录中新增视频（顶层目录 mtime 不会变化）
        create_test_video(&sub.join("two.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);

        scan_videos_sync(path, &app_state, true).expect("再次扫描应成功");
        assert_eq!(
            app_state.shared_videos().len(),
            2,
            "子目录新增文件后缓存应失效并重新扫描"
        );

        let _ = fs::remove_dir_all(&scan_dir);
    }
}
