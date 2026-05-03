use serde::Serialize;
use std::fmt;

/// 应用错误类型
///
/// 使用结构化错误替代 String，便于前端根据错误类型进行针对性处理
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", content = "message")]
pub enum AppError {
    InvalidPath(String),
    ScanCancelled,
    ServerAlreadyRunning,
    ServerNotRunning,
    IoError(String),
    PasswordError(String),
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidPath(msg) => write!(f, "路径错误: {}", msg),
            AppError::ScanCancelled => write!(f, "扫描已取消"),
            AppError::ServerAlreadyRunning => write!(f, "服务器已在运行"),
            AppError::ServerNotRunning => write!(f, "服务器未运行"),
            AppError::IoError(msg) => write!(f, "IO 错误: {}", msg),
            AppError::PasswordError(msg) => write!(f, "密码错误: {}", msg),
            AppError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AppError {}

// 从 &str 自动转换
impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        AppError::Other(msg.to_string())
    }
}

// 从 String 自动转换
impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError::Other(msg)
    }
}

// 从 std::io::Error 自动转换
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IoError(err.to_string())
    }
}

// 从 serde_json::Error 自动转换
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Other(format!("JSON 序列化错误: {}", err))
    }
}
