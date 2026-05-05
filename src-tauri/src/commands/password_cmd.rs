//! 密码保护管理命令
//!
//! 提供密码状态查询、启用/禁用、设置、验证、随机生成和重置等 Tauri IPC 命令。
//! 这些命令是对 password 模块功能的薄封装，负责参数转换和错误映射。

use crate::error::AppError;
use crate::password;
use crate::password::PasswordStatus;

/// 获取密码保护状态（是否启用 + 是否已设置）
#[tauri::command]
pub fn get_password_status() -> PasswordStatus {
    password::get_password_status()
}

/// 启用或禁用密码保护（启用前必须已设置密码）
#[tauri::command]
pub fn set_password_enabled(enabled: bool) -> Result<(), AppError> {
    if enabled && !password::has_password_set() {
        return Err(AppError::PasswordError(
            "请先设置密码再启用密码保护".to_string(),
        ));
    }
    password::set_password_enabled(enabled);
    Ok(())
}

/// 设置密码（4 位数字）
#[tauri::command]
pub fn set_password(password: String) -> Result<(), AppError> {
    password::set_password(&password).map_err(AppError::PasswordError)
}

/// 验证密码是否正确
#[tauri::command]
pub fn verify_password_cmd(password: String) -> Result<bool, AppError> {
    password::verify_password(&password).map_err(AppError::PasswordError)
}

/// 生成 4 位随机数字密码
#[tauri::command]
pub fn generate_random_password() -> String {
    password::generate_random_password()
}

/// 重置密码（清除密码并禁用保护）
#[tauri::command]
pub fn reset_password() -> Result<(), AppError> {
    password::reset_password();
    Ok(())
}
