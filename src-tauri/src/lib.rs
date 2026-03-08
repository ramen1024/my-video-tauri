use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
pub struct VideoFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub created: Option<String>,
    pub extension: String,
}

fn format_system_time(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[tauri::command]
fn scan_videos(folder_path: String) -> Result<Vec<VideoFile>, String> {
    let path = Path::new(&folder_path);
    if !path.exists() {
        return Err("文件夹不存在".to_string());
    }
    if !path.is_dir() {
        return Err("路径不是文件夹".to_string());
    }

    let video_extensions = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg",
    ];

    let mut videos: Vec<VideoFile> = Vec::new();

    for entry in WalkDir::new(&folder_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if video_extensions.contains(&ext_lower.as_str()) {
                    let metadata = fs::metadata(path).ok();
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let created = metadata
                        .and_then(|m| m.created().ok())
                        .map(format_system_time);

                    videos.push(VideoFile {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.to_string_lossy().to_string(),
                        size,
                        created,
                        extension: ext_lower,
                    });
                }
            }
        }
    }

    videos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(videos)
}

#[tauri::command]
fn play_video(file_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &file_path])
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("无法打开视频: {}", e))?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![scan_videos, play_video])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
