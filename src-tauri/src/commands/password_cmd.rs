use crate::error::AppError;
use crate::password;
use crate::password::PasswordStatus;

#[tauri::command]
pub fn get_password_status() -> PasswordStatus {
    password::get_password_status()
}

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

#[tauri::command]
pub fn set_password(password: String) -> Result<(), AppError> {
    password::set_password(&password).map_err(AppError::PasswordError)
}

#[tauri::command]
pub fn verify_password_cmd(password: String) -> Result<bool, AppError> {
    password::verify_password(&password).map_err(AppError::PasswordError)
}

#[tauri::command]
pub fn generate_random_password() -> String {
    password::generate_random_password()
}

#[tauri::command]
pub fn reset_password() -> Result<(), AppError> {
    password::reset_password();
    Ok(())
}
