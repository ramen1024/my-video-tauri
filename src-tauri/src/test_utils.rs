//! 测试专用助手（仅在 cargo test 下编译，不进入发布产物）
//!
//! 集中各模块单元测试重复使用的临时目录与样例数据构造逻辑，
//! 替代此前复制到多个 `mod tests` 中的同名函数。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 在系统临时目录下创建带唯一后缀的目录，测试结束时由调用方自行清理
pub fn make_temp_dir(prefix: &str) -> PathBuf {
    let name = format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(name);
    fs::create_dir_all(&path).expect("创建临时目录失败");
    path
}

/// 写入指定大小的全零文件，用于构造尺寸达标（≥ MIN_VIDEO_FILE_SIZE_BYTES）的假视频
pub fn create_test_video(path: &Path, size: u64) {
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

/// 构造指定相对路径、大小、修改时间的样例视频条目
///
/// 文件名取自相对路径的最后一段，扩展名固定为 mp4。
pub fn sample_video(relative_path: &str, size: u64, modified: Option<&str>) -> crate::VideoFile {
    crate::VideoFile {
        name: relative_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(relative_path)
            .to_string(),
        path: format!("C:\\videos\\{}", relative_path),
        relative_path: relative_path.to_string(),
        size,
        modified: modified.map(|m| m.to_string()),
        extension: "mp4".to_string(),
    }
}
