use std::sync::LazyLock;

use parking_lot::RwLock;
use rand::Rng;
use sha2::{Sha256, Digest};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static PASSWORD_ENABLED: AtomicBool = AtomicBool::new(false);

static PASSWORD_HASH: LazyLock<Arc<RwLock<Option<String>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

static CONFIG_DIR: LazyLock<RwLock<Option<std::path::PathBuf>>> =
    LazyLock::new(|| RwLock::new(None));

static SESSIONS: LazyLock<Arc<RwLock<HashMap<String, i64>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

static FAILED_ATTEMPTS: LazyLock<Arc<RwLock<HashMap<String, FailedAttempt>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

const SESSION_DURATION_SECS: i64 = 3600;
const MAX_FAILED_ATTEMPTS: u32 = 3;
const LOCK_DURATION_SECS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordStatus {
    pub enabled: bool,
    pub has_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PasswordConfig {
    password_hash: Option<String>,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct FailedAttempt {
    count: u32,
    locked_until: i64,
}

fn current_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

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

pub fn set_config_dir(path: std::path::PathBuf) {
    let mut dir = CONFIG_DIR.write();
    *dir = Some(path);
}

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

pub fn is_password_enabled() -> bool {
    PASSWORD_ENABLED.load(Ordering::SeqCst)
}

pub fn set_password_enabled(enabled: bool) {
    PASSWORD_ENABLED.store(enabled, Ordering::SeqCst);
    save_password_config();
}

pub fn has_password_set() -> bool {
    PASSWORD_HASH.read().is_some()
}

pub fn get_password_status() -> PasswordStatus {
    PasswordStatus {
        enabled: is_password_enabled(),
        has_password: has_password_set(),
    }
}

pub fn generate_random_password() -> String {
    let mut rng = rand::rng();
    format!("{:04}", rng.random_range(0..10000))
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

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

pub fn create_session() -> String {
    let token = generate_session_token();
    let expiry = current_timestamp() + SESSION_DURATION_SECS;
    SESSIONS.write().insert(token.clone(), expiry);
    token
}

fn generate_session_token() -> String {
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let byte = rng.random::<u8>();
            format!("{:02x}", byte)
        })
        .collect()
}

pub fn validate_session(token: &str) -> bool {
    let sessions = SESSIONS.read();
    if let Some(expiry) = sessions.get(token) {
        if *expiry > current_timestamp() {
            return true;
        }
    }
    false
}

pub fn cleanup_expired_sessions() {
    let now = current_timestamp();
    let mut sessions = SESSIONS.write();
    sessions.retain(|_, expiry| *expiry > now);
}

pub fn start_cleanup_thread() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(600));
        cleanup_expired_sessions();
    });
}

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

pub fn clear_failed_attempts(ip: &str) {
    let mut attempts = FAILED_ATTEMPTS.write();
    attempts.remove(ip);
}

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

pub fn check_web_auth(cookie_header: &str) -> bool {
    if let Some(token) = extract_session_token(cookie_header) {
        validate_session(&token)
    } else {
        false
    }
}

fn extract_session_token(cookie_header: &str) -> Option<String> {
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if cookie.starts_with("session_token=") {
            return Some(cookie[14..].to_string());
        }
    }
    None
}
