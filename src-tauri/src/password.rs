//! 密码管理模块
//!
//! 提供局域网共享服务的密码保护功能，包括：
//! - 4 位数字密码的设置、验证和重置
//! - 基于 session token 的访问认证
//! - 基于 IP 的登录频率限制（防止暴力破解）
//! - 密码配置的持久化存储（JSON 文件）

use std::sync::LazyLock;

use parking_lot::RwLock;
use rand::Rng;
use sha2::{Sha256, Digest};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 密码保护是否启用
static PASSWORD_ENABLED: AtomicBool = AtomicBool::new(false);

/// 密码的 SHA-256 哈希值（明文不存储）
static PASSWORD_HASH: LazyLock<Arc<RwLock<Option<String>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

/// 密码配置文件的存储目录（由 Tauri setup 阶段设置）
static CONFIG_DIR: LazyLock<RwLock<Option<std::path::PathBuf>>> =
    LazyLock::new(|| RwLock::new(None));

/// 活跃的 session token 及其过期时间戳
static SESSIONS: LazyLock<Arc<RwLock<HashMap<String, i64>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

/// 各 IP 的登录失败记录，用于频率限制
static FAILED_ATTEMPTS: LazyLock<Arc<RwLock<HashMap<String, FailedAttempt>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Session 有效时长（秒）
const SESSION_DURATION_SECS: i64 = 3600;
/// 最大连续失败次数，超过后锁定 IP
const MAX_FAILED_ATTEMPTS: u32 = 3;
/// IP 锁定时长（秒）
const LOCK_DURATION_SECS: i64 = 30;

/// 密码保护状态，返回给前端显示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordStatus {
    /// 密码保护是否已启用
    pub enabled: bool,
    /// 是否已设置密码
    pub has_password: bool,
}

/// 密码配置的持久化结构，序列化为 JSON 存储
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PasswordConfig {
    password_hash: Option<String>,
    enabled: bool,
}

/// 单个 IP 的登录失败记录
#[derive(Debug, Clone)]
struct FailedAttempt {
    /// 连续失败次数
    count: u32,
    /// 锁定截止时间戳（0 表示未锁定）
    locked_until: i64,
}

/// 获取当前 UTC 时间戳（秒）
fn current_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 获取密码配置文件路径
///
/// 优先使用 Tauri 设置的应用数据目录，回退到可执行文件所在目录
fn config_path() -> Result<std::path::PathBuf, String> {
    let dir_guard = CONFIG_DIR.read();
    if let Some(dir) = dir_guard.as_ref() {
        let path = dir.join("password_config.json");
        if !dir.exists() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("创建配置目录失败: {}", e))?;
        }
        return Ok(path);
    }
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    Ok(path.join("password_config.json"))
}

/// 设置密码配置文件的存储目录（由 Tauri setup 阶段调用）
pub fn set_config_dir(path: std::path::PathBuf) {
    let mut dir = CONFIG_DIR.write();
    *dir = Some(path);
}

/// 从配置文件加载密码设置，应用启动时调用
pub fn load_password_config() {
    match config_path() {
        Ok(path) => {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<PasswordConfig>(&content) {
                    if let Some(hash) = config.password_hash {
                        let mut stored_hash = PASSWORD_HASH.write();
                        *stored_hash = Some(hash);
                    }
                    PASSWORD_ENABLED.store(config.enabled, Ordering::SeqCst);
                }
            }
        }
        Err(e) => log::error!("[密码配置] 获取配置路径失败: {}", e),
    }
}

/// 将当前密码配置保存到 JSON 文件
fn save_password_config() {
    let config = PasswordConfig {
        password_hash: PASSWORD_HASH.read().clone(),
        enabled: PASSWORD_ENABLED.load(Ordering::SeqCst),
    };
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        match config_path() {
            Ok(path) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::error!("[密码配置] 保存失败: {}", e);
                }
            }
            Err(e) => log::error!("[密码配置] 获取配置路径失败: {}", e),
        }
    }
}

/// 查询密码保护是否已启用
pub fn is_password_enabled() -> bool {
    PASSWORD_ENABLED.load(Ordering::SeqCst)
}

/// 启用或禁用密码保护，并持久化配置
pub fn set_password_enabled(enabled: bool) {
    PASSWORD_ENABLED.store(enabled, Ordering::SeqCst);
    save_password_config();
}

/// 检查是否已设置密码
pub fn has_password_set() -> bool {
    PASSWORD_HASH.read().is_some()
}

/// 获取密码保护状态（是否启用 + 是否已设置）
pub fn get_password_status() -> PasswordStatus {
    PasswordStatus {
        enabled: is_password_enabled(),
        has_password: has_password_set(),
    }
}

/// 生成 4 位随机数字密码（0000-9999）
pub fn generate_random_password() -> String {
    let mut rng = rand::rng();
    format!("{:04}", rng.random_range(0..10000))
}

/// 对密码进行 SHA-256 哈希
fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// 设置密码（必须为 4 位纯数字），哈希后存储
pub fn set_password(password: &str) -> Result<(), String> {
    if password.len() != 4 {
        return Err("密码必须是4位数字".to_string());
    }
    if !password.chars().all(|c| c.is_ascii_digit()) {
        return Err("密码只能包含数字0-9".to_string());
    }

    let hash = hash_password(password);

    {
        let mut stored_hash = PASSWORD_HASH.write();
        *stored_hash = Some(hash);
    }
    save_password_config();
    Ok(())
}

/// 验证密码是否正确（与存储的哈希值比对）
pub fn verify_password(password: &str) -> Result<bool, String> {
    let hash_guard = PASSWORD_HASH.read();
    match hash_guard.as_ref() {
        Some(hash) => {
            let computed = hash_password(password);
            Ok(computed == *hash)
        }
        None => Err("未设置密码".to_string()),
    }
}

/// 重置密码：清除哈希、所有 session，并禁用密码保护
pub fn reset_password() {
    {
        let mut stored_hash = PASSWORD_HASH.write();
        *stored_hash = None;
    }
    {
        let mut sessions = SESSIONS.write();
        sessions.clear();
    }
    PASSWORD_ENABLED.store(false, Ordering::SeqCst);
    save_password_config();
}

/// 创建新的 session token，有效期 SESSION_DURATION_SECS 秒
pub fn create_session() -> String {
    let token = generate_session_token();
    let expiry = current_timestamp() + SESSION_DURATION_SECS;
    SESSIONS.write().insert(token.clone(), expiry);
    token
}

/// 生成 64 字符的随机十六进制 session token
fn generate_session_token() -> String {
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let byte = rng.random::<u8>();
            format!("{:02x}", byte)
        })
        .collect()
}

/// 验证 session token 是否有效且未过期
pub fn validate_session(token: &str) -> bool {
    let sessions = SESSIONS.read();
    if let Some(expiry) = sessions.get(token) {
        if *expiry > current_timestamp() {
            return true;
        }
    }
    false
}

/// 清理所有已过期的 session
pub fn cleanup_expired_sessions() {
    let now = current_timestamp();
    let mut sessions = SESSIONS.write();
    sessions.retain(|_, expiry| *expiry > now);
}

/// 启动后台线程，定期清理过期 session（每 10 分钟）
pub fn start_cleanup_thread() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(600));
        cleanup_expired_sessions();
    });
}

/// 检查指定 IP 是否被频率限制锁定
///
/// 连续失败 MAX_FAILED_ATTEMPTS 次后，IP 将被锁定 LOCK_DURATION_SECS 秒
pub fn check_rate_limit(ip: &str) -> Result<(), String> {
    let mut attempts = FAILED_ATTEMPTS.write();
    let now = current_timestamp();

    if let Some(attempt) = attempts.get(ip) {
        if attempt.locked_until > now {
            let remaining = attempt.locked_until - now;
            return Err(format!("访问已锁定，请{}秒后重试", remaining));
        }
        if now > attempt.locked_until && attempt.count >= MAX_FAILED_ATTEMPTS {
            attempts.remove(ip);
        }
    }

    Ok(())
}

/// 记录一次登录失败，达到阈值后锁定该 IP
pub fn record_failed_attempt(ip: &str) {
    let mut attempts = FAILED_ATTEMPTS.write();
    let now = current_timestamp();

    let attempt = attempts.entry(ip.to_string()).or_insert(FailedAttempt {
        count: 0,
        locked_until: 0,
    });

    attempt.count += 1;

    if attempt.count >= MAX_FAILED_ATTEMPTS {
        attempt.locked_until = now + LOCK_DURATION_SECS;
        attempt.count = 0;
    }
}

/// 清除指定 IP 的失败记录（登录成功后调用）
pub fn clear_failed_attempts(ip: &str) {
    let mut attempts = FAILED_ATTEMPTS.write();
    attempts.remove(ip);
}

/// Web 请求认证入口：验证密码并创建 session
///
/// 流程：频率限制检查 → 密码验证 → 创建 session token
pub fn authenticate_web_request(ip: &str, password: &str) -> Result<String, String> {
    check_rate_limit(ip)?;

    match verify_password(password) {
        Ok(true) => {
            clear_failed_attempts(ip);
            let token = create_session();
            Ok(token)
        }
        Ok(false) => {
            record_failed_attempt(ip);
            Err("密码错误，请重试".to_string())
        }
        Err(e) => Err(e),
    }
}

/// 检查 HTTP 请求的 Cookie 中是否包含有效的 session token
pub fn check_web_auth(cookie_header: &str) -> bool {
    if let Some(token) = extract_session_token(cookie_header) {
        validate_session(&token)
    } else {
        false
    }
}

/// 从 Cookie 请求头中提取 session_token 的值
fn extract_session_token(cookie_header: &str) -> Option<String> {
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if cookie.starts_with("session_token=") {
            return Some(cookie[14..].to_string());
        }
    }
    None
}
