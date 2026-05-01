use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static PASSWORD_ENABLED: AtomicBool = AtomicBool::new(false);

static PASSWORD_HASH: once_cell::sync::Lazy<Arc<RwLock<Option<String>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(None)));

static SESSIONS: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, i64>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

static FAILED_ATTEMPTS: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, FailedAttempt>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

const SESSION_DURATION_SECS: i64 = 3600;
const MAX_FAILED_ATTEMPTS: u32 = 3;
const LOCK_DURATION_SECS: i64 = 30;
const HASH_ITERATIONS: u32 = 10000;

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

fn config_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    path.join("password_config.json")
}

pub fn load_password_config() {
    let path = config_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str::<PasswordConfig>(&content) {
            if let Some(hash) = config.password_hash {
                let mut stored = PASSWORD_HASH.write();
                *stored = Some(hash);
            }
            PASSWORD_ENABLED.store(config.enabled, Ordering::SeqCst);
            println!("[密码配置] 已从文件加载 - 启用状态: {}", config.enabled);
        }
    }
}

fn save_password_config() {
    let config = PasswordConfig {
        password_hash: PASSWORD_HASH.read().clone(),
        enabled: PASSWORD_ENABLED.load(Ordering::SeqCst),
    };
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        let path = config_path();
        if let Err(e) = std::fs::write(&path, json) {
            println!("[密码配置] 保存失败: {}", e);
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
    let mut rng = rand::thread_rng();
    format!("{:04}", rng.gen_range(0..10000))
}

fn generate_salt() -> String {
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

fn hash_password(password: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}{}", salt, password).as_bytes());
    let mut result = hasher.finalize();

    for _ in 1..HASH_ITERATIONS {
        let mut hasher = Sha256::new();
        hasher.update(&result);
        result = hasher.finalize();
    }

    format!("{}${}", salt, hex::encode(result))
}

fn verify_hash(password: &str, stored_hash: &str) -> bool {
    let parts: Vec<&str> = stored_hash.splitn(2, '$').collect();
    if parts.len() != 2 {
        return false;
    }
    let salt = parts[0];
    let computed = hash_password(password, salt);
    constant_time_eq(&computed, stored_hash)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

pub fn set_password(password: &str) -> Result<(), String> {
    if password.len() != 4 {
        return Err("密码必须是4位数字".to_string());
    }
    if !password.chars().all(|c| c.is_ascii_digit()) {
        return Err("密码只能包含数字0-9".to_string());
    }

    let salt = generate_salt();
    let hashed = hash_password(password, &salt);
    {
        let mut stored = PASSWORD_HASH.write();
        *stored = Some(hashed);
    }
    save_password_config();
    Ok(())
}

pub fn verify_password(password: &str) -> Result<bool, String> {
    let stored = PASSWORD_HASH.read();
    match stored.as_ref() {
        Some(hashed) => Ok(verify_hash(password, hashed)),
        None => Err("未设置密码".to_string()),
    }
}

pub fn reset_password() {
    {
        let mut stored = PASSWORD_HASH.write();
        *stored = None;
        let mut sessions = SESSIONS.write();
        sessions.clear();
        PASSWORD_ENABLED.store(false, Ordering::SeqCst);
    }
    save_password_config();
}

pub fn create_session() -> String {
    let token = generate_session_token();
    let expiry = current_timestamp() + SESSION_DURATION_SECS;
    SESSIONS.write().insert(token.clone(), expiry);
    token
}

fn generate_session_token() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let byte = rng.gen::<u8>();
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
        println!(
            "IP {} 连续{}次验证失败，锁定{}秒",
            ip, MAX_FAILED_ATTEMPTS, LOCK_DURATION_SECS
        );
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
