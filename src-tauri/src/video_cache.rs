//! 视频扫描结果缓存模块
//!
//! 将扫描得到的视频列表按文件夹维度持久化到磁盘，避免每次启动共享服务器
//! 都重新执行全量扫描。缓存以文件夹的修改时间（mtime）作为失效依据。

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::VideoFile;

/// 单个文件夹的缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoCacheEntry {
    /// 缓存的视频列表
    pub videos: Vec<VideoFile>,
    /// 缓存时文件夹的修改时间（秒级 UNIX 时间戳）
    pub folder_modified_time: u64,
    /// 缓存写入时间（秒级 UNIX 时间戳）
    pub cached_at: i64,
}

/// 视频扫描缓存
///
/// 内部维护一个内存中的缓存映射，并异步持久化到磁盘 JSON 文件。
pub struct VideoCache {
    /// 缓存文件在磁盘上的完整路径
    cache_file_path: PathBuf,
    /// 已加载的缓存条目
    entries: HashMap<String, VideoCacheEntry>,
}

impl VideoCache {
    /// 使用指定缓存目录创建缓存实例
    ///
    /// 缓存文件固定为 `<cache_dir>/video_cache.json`，初始化时不会立即加载。
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_file_path: cache_dir.join("video_cache.json"),
            entries: HashMap::new(),
        }
    }

    /// 切换缓存目录并尝试从新的路径加载缓存
    pub fn set_cache_dir(&mut self, cache_dir: PathBuf) {
        self.cache_file_path = cache_dir.join("video_cache.json");
        if let Err(e) = self.load() {
            log::debug!("[视频缓存] 从新目录加载缓存失败: {}", e);
        }
    }

    /// 从磁盘 JSON 文件加载缓存
    pub fn load(&mut self) -> Result<(), String> {
        if !self.cache_file_path.exists() {
            self.entries.clear();
            return Ok(());
        }

        let content = fs::read_to_string(&self.cache_file_path)
            .map_err(|e| format!("读取缓存文件失败: {}", e))?;

        if content.trim().is_empty() {
            self.entries.clear();
            return Ok(());
        }

        self.entries = serde_json::from_str(&content)
            .map_err(|e| format!("解析缓存文件失败: {}", e))?;

        log::info!(
            "[视频缓存] 已加载 {} 条缓存记录",
            self.entries.len()
        );
        Ok(())
    }

    /// 将当前缓存写入磁盘 JSON 文件
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.cache_file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建缓存目录失败: {}", e))?;
        }

        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| format!("序列化缓存失败: {}", e))?;

        let mut file = fs::File::create(&self.cache_file_path)
            .map_err(|e| format!("创建缓存文件失败: {}", e))?;

        file.write_all(json.as_bytes())
            .map_err(|e| format!("写入缓存文件失败: {}", e))?;

        Ok(())
    }

    /// 获取指定文件夹的有效缓存
    ///
    /// 仅当缓存条目的 `folder_modified_time` 与当前 mtime 一致时返回。
    pub fn get(
        &self,
        folder_path: &str,
        current_modified_time: u64,
    ) -> Option<&VideoCacheEntry> {
        let entry = self.entries.get(folder_path)?;
        if entry.folder_modified_time == current_modified_time {
            Some(entry)
        } else {
            None
        }
    }

    /// 更新指定文件夹的缓存并持久化到磁盘
    pub fn set(
        &mut self,
        folder_path: String,
        entry: VideoCacheEntry,
    ) -> Result<(), String> {
        self.entries.insert(folder_path, entry);
        self.save()
    }

    /// 删除指定文件夹的缓存条目并持久化
    pub fn invalidate(&mut self, folder_path: &str) -> Result<(), String> {
        self.entries.remove(folder_path);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::VideoFile;
    use std::fs;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let mut name = prefix.to_string();
        name.push_str("_");
        name.push_str(&std::process::id().to_string());
        name.push_str("_");
        name.push_str(&std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string());
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    fn sample_entry(folder_modified_time: u64) -> VideoCacheEntry {
        VideoCacheEntry {
            videos: vec![VideoFile {
                name: "test.mp4".to_string(),
                path: "/tmp/test.mp4".to_string(),
                relative_path: "test.mp4".to_string(),
                size: 1234,
                modified: None,
                extension: "mp4".to_string(),
            }],
            folder_modified_time,
            cached_at: 0,
        }
    }

    #[test]
    fn test_cache_get_when_mtime_matches() {
        let cache_dir = make_temp_dir("cache_match");
        let mut cache = VideoCache::new(cache_dir.clone());
        cache
            .set("/fake/folder".to_string(), sample_entry(42))
            .expect("保存缓存应成功");

        let result = cache.get("/fake/folder", 42);
        assert!(result.is_some(), "mtime 匹配时应返回缓存");
        assert_eq!(result.unwrap().folder_modified_time, 42);
        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn test_cache_get_when_mtime_differs() {
        let cache_dir = make_temp_dir("cache_diff");
        let mut cache = VideoCache::new(cache_dir.clone());
        cache
            .set("/fake/folder".to_string(), sample_entry(42))
            .expect("保存缓存应成功");

        let result = cache.get("/fake/folder", 43);
        assert!(result.is_none(), "mtime 不同时应返回 None");
        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn test_cache_save_and_load() {
        let cache_dir = make_temp_dir("cache_load");
        let folder = "/fake/folder";

        {
            let mut cache = VideoCache::new(cache_dir.clone());
            cache
                .set(folder.to_string(), sample_entry(100))
                .expect("保存缓存应成功");
        }

        {
            let mut cache = VideoCache::new(cache_dir.clone());
            cache.load().expect("加载缓存应成功");
            let result = cache.get(folder, 100);
            assert!(result.is_some(), "加载后应能获取到缓存");
            let entry = result.unwrap();
            assert_eq!(entry.folder_modified_time, 100);
            assert_eq!(entry.videos.len(), 1);
            assert_eq!(entry.videos[0].name, "test.mp4");
        }

        let _ = fs::remove_dir_all(&cache_dir);
    }
}
