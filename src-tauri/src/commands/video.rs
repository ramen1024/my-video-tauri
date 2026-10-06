//! 视频扫描与管理命令
//!
//! 提供视频文件的扫描、获取、播放和取消扫描等 Tauri IPC 命令。
//! 扫描在单次目录遍历中完成过滤与元数据提取，结果写入 AppState；
//! 支持通过 AppState 中的取消标志中止扫描。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use tauri::State;
use walkdir::WalkDir;

use crate::constants::{is_supported_video_extension, MIN_VIDEO_FILE_SIZE_BYTES};
use crate::error::AppError;
use crate::models::{ScanReport, VideoFile};
use crate::utils::{allow_shared_folder_asset_scope, format_system_time, is_root_directory};
use crate::AppState;

/// 扫描指定文件夹中的视频文件
///
/// 在阻塞线程中执行扫描，完成后返回本次扫描报告（视频列表经 `get_shared_videos` 获取）。
/// 扫描结果写入 AppState 中的 shared_videos 和 shared_folder_path。
#[tauri::command]
pub async fn scan_videos(
    folder_path: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ScanReport, AppError> {
    let app_state = state.inner().clone();
    let scan_folder = folder_path.clone();

    let report = tauri::async_runtime::spawn_blocking(move || -> Result<ScanReport, AppError> {
        scan_videos_sync(scan_folder, &app_state)
    })
    .await
    .map_err(|e| AppError::Other(format!("扫描任务执行失败: {}", e)))??;

    // 桌面端通过 asset 协议播放视频，扫描成功后放行该文件夹
    allow_shared_folder_asset_scope(&app, &folder_path);

    Ok(report)
}

/// RAII 守卫：持有扫描互斥标记，作用域结束时自动释放
///
/// 保证桌面扫描与网页刷新互斥，所有返回路径（错误、取消、成功）都会释放标记。
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

/// 遍历条目的转换结果
enum EntryOutcome {
    /// 纳入列表
    Included(VideoFile),
    /// 因小于最小体积被跳过（携带文件名与大小，用于写入扫描报告）
    TooSmall { name: String, size: u64 },
    /// 与本应用无关（目录、非视频扩展名等），无需记录
    Ignored,
}

/// 将遍历条目转换为 [`VideoFile`]，应用文件类型、扩展名与最小大小过滤
///
/// 返回 [`EntryOutcome`] 而不是 `Option`：大小过滤是**用户可感知**的丢弃
/// （用户放进去了文件、列表里却没有），必须把原因带出去报告，
/// 不能像非视频扩展名那样静默忽略。
fn entry_to_video_file(entry: &walkdir::DirEntry, base_path: &Path) -> EntryOutcome {
    if !entry.file_type().is_file() {
        return EntryOutcome::Ignored;
    }

    let ext = entry
        .path()
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    let is_supported = ext.as_deref().is_some_and(is_supported_video_extension);
    if !is_supported {
        return EntryOutcome::Ignored;
    }

    let metadata = match entry
        .metadata()
        .ok()
        .or_else(|| fs::metadata(entry.path()).ok())
    {
        Some(metadata) => metadata,
        // 元数据读取失败（权限/竞态删除）：无法判定体积，按忽略处理，
        // 但不能算"过小"，否则报告会把权限问题误报成体积问题
        None => return EntryOutcome::Ignored,
    };
    let size = metadata.len();

    let name = entry
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if size < MIN_VIDEO_FILE_SIZE_BYTES {
        return EntryOutcome::TooSmall { name, size };
    }

    EntryOutcome::Included(VideoFile {
        name,
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

/// 同步执行视频扫描（在 spawn_blocking 中调用）
///
/// 流程：验证路径 → 单次遍历过滤并提取元数据 → 按名称排序 → 写入 AppState。
/// 返回本次扫描报告，其中记录了被跳过的过小文件及其原因。
///
/// # 参数
/// - `folder_path`: 要扫描的文件夹路径
/// - `app_state`: 应用状态
pub(crate) fn scan_videos_sync(
    folder_path: String,
    app_state: &AppState,
) -> Result<ScanReport, AppError> {
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

    // 单次遍历：过滤 + 提取元数据 + 构建结果；被跳过的过小文件写入报告
    let mut videos: Vec<VideoFile> = Vec::new();
    let mut skipped: Vec<(String, u64)> = Vec::new();

    for entry in WalkDir::new(&folder_path)
        .follow_links(false)
        .into_iter()
        .take_while(|_| !app_state.is_scan_cancelled())
        .filter_map(|e| e.ok())
    {
        match entry_to_video_file(&entry, &base_path) {
            EntryOutcome::Included(video) => videos.push(video),
            EntryOutcome::TooSmall { name, size } => skipped.push((name, size)),
            EntryOutcome::Ignored => {}
        }
    }

    if app_state.is_scan_cancelled() {
        return Err(AppError::ScanCancelled("扫描已取消".to_string()));
    }

    videos.sort_by_cached_key(|v| v.name.to_lowercase());

    let video_count = videos.len();
    app_state.set_shared_videos(videos);
    app_state.set_shared_folder_path(folder_path);

    let mut report = ScanReport {
        total: video_count,
        ..Default::default()
    };
    for (name, size) in skipped {
        report.record_skipped_small(name, size);
    }

    if report.has_skipped() {
        log::info!(
            "[扫描] 完成，共 {} 个视频；跳过 {} 个小于 {} 字节的文件",
            video_count,
            report.skipped_small_count,
            MIN_VIDEO_FILE_SIZE_BYTES
        );
    } else {
        log::info!("[扫描] 完成，共 {} 个视频", video_count);
    }

    Ok(report)
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

    if !ext.as_deref().is_some_and(is_supported_video_extension) {
        return Err(AppError::InvalidPath("不允许打开非视频文件".to_string()));
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
    use crate::test_utils::{create_test_video, make_temp_dir};
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
        scan_videos_sync(scan_dir.to_string_lossy().to_string(), &app_state)
            .expect("扫描应成功完成");

        let videos = app_state.shared_videos();
        assert_eq!(videos.len(), 1, "应只包含真实视频文件");
        assert_eq!(videos[0].name, "real.mp4", "应忽略符号链接指向的文件");

        let _ = fs::remove_dir_all(&scan_dir);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn test_scan_videos_filters_by_extension_and_size() {
        let scan_dir = make_temp_dir("scan_filter");
        let sub = scan_dir.join("sub");
        fs::create_dir_all(&sub).expect("创建子目录失败");

        // 达标视频：顶层与子目录各一个
        create_test_video(&scan_dir.join("big.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);
        create_test_video(&sub.join("nested.mkv"), MIN_VIDEO_FILE_SIZE_BYTES);
        // 应被过滤：小于最小体积
        create_test_video(&scan_dir.join("small.mp4"), MIN_VIDEO_FILE_SIZE_BYTES - 1);
        // 应被过滤：扩展名不受支持
        create_test_video(&scan_dir.join("movie.txt"), MIN_VIDEO_FILE_SIZE_BYTES);
        // 应被过滤：不是文件
        fs::create_dir_all(scan_dir.join("fake.mp4")).expect("创建同名目录失败");

        let app_state = AppState::new();
        let report = scan_videos_sync(scan_dir.to_string_lossy().to_string(), &app_state)
            .expect("扫描应成功");

        let videos = app_state.shared_videos();
        let names: Vec<&str> = videos.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["big.mp4", "nested.mkv"],
            "应只保留体积达标且扩展名受支持的文件，并按名称升序排列"
        );

        // 报告必须解释"为什么少了一个文件"：过小的记名、非视频的不报
        assert_eq!(report.total, 2, "报告中的总数应与列表一致");
        assert_eq!(
            report.skipped_small_count, 1,
            "只有 small.mp4 属于'过小'；movie.txt 是非视频、fake.mp4 是目录，都不该计入"
        );
        assert_eq!(report.skipped_small.len(), 1);
        assert_eq!(report.skipped_small[0].name, "small.mp4");
        assert_eq!(report.skipped_small[0].size, MIN_VIDEO_FILE_SIZE_BYTES - 1);
        assert!(!report.skipped_small_truncated);

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn test_scan_report_truncates_skipped_list_but_keeps_count() {
        // 小文件极多的目录不该让报告本身膨胀成新的问题源
        let scan_dir = make_temp_dir("scan_report_truncate");
        let small_count = crate::constants::MAX_SKIPPED_FILES_REPORTED + 5;
        for i in 0..small_count {
            create_test_video(&scan_dir.join(format!("tiny_{i}.mp4")), 1);
        }

        let app_state = AppState::new();
        let report = scan_videos_sync(scan_dir.to_string_lossy().to_string(), &app_state)
            .expect("扫描应成功");

        assert_eq!(report.total, 0, "全部文件都过小，列表应为空");
        assert_eq!(
            report.skipped_small_count, small_count,
            "计数必须是完整数量，不能因为截断列表而丢失"
        );
        assert_eq!(
            report.skipped_small.len(),
            crate::constants::MAX_SKIPPED_FILES_REPORTED,
            "明细列表应被截断到上限"
        );
        assert!(
            report.skipped_small_truncated,
            "截断后必须给出标记，否则界面会把'列出的部分'当成全部"
        );

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn test_scan_report_is_empty_when_nothing_skipped() {
        let scan_dir = make_temp_dir("scan_report_clean");
        create_test_video(&scan_dir.join("ok.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);

        let app_state = AppState::new();
        let report = scan_videos_sync(scan_dir.to_string_lossy().to_string(), &app_state)
            .expect("扫描应成功");

        assert_eq!(report.total, 1);
        assert!(!report.has_skipped(), "没有跳过的文件时不应产生提示");
        assert_eq!(report.skipped_small_count, 0);
        assert!(report.skipped_small.is_empty());

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn test_scan_videos_rescans_subdirectory_changes() {
        // 每次扫描都重新遍历目录，子目录新增文件（不改变顶层目录 mtime）必须被感知
        let scan_dir = make_temp_dir("scan_rescan");
        let sub = scan_dir.join("sub");
        fs::create_dir_all(&sub).expect("创建子目录失败");
        create_test_video(&sub.join("one.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);

        let app_state = AppState::new();
        let path = scan_dir.to_string_lossy().to_string();

        scan_videos_sync(path.clone(), &app_state).expect("首次扫描应成功");
        assert_eq!(
            app_state.shared_videos().len(),
            1,
            "首次扫描应发现 1 个视频"
        );

        create_test_video(&sub.join("two.mp4"), MIN_VIDEO_FILE_SIZE_BYTES);

        scan_videos_sync(path, &app_state).expect("再次扫描应成功");
        assert_eq!(
            app_state.shared_videos().len(),
            2,
            "子目录新增文件后应被再次扫描发现"
        );

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn test_scan_videos_rejects_root_directory() {
        let app_state = AppState::new();
        #[cfg(windows)]
        let root = "C:\\".to_string();
        #[cfg(unix)]
        let root = "/".to_string();

        let result = scan_videos_sync(root, &app_state);
        assert!(
            matches!(result, Err(AppError::InvalidPath(ref msg)) if msg.contains("根目录")),
            "磁盘根目录应被拒绝扫描"
        );
    }
}
