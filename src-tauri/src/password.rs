//! 密码管理模块
//!
//! 提供局域网共享服务的密码保护功能，包括：
//! - 4 位数字密码的设置、验证和重置
//! - 基于 session token 的访问认证
//! - 基于 IP 的登录频率限制（防止暴力破解）
//! - 密码配置的持久化存储（JSON 文件）

use std::sync::LazyLock;

use parking_lot::RwLock;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::constants::{
    LOCK_DURATION_SECS, MAX_FAILED_ATTEMPTS, SESSION_CLEANUP_INTERVAL_SECS, SESSION_DURATION_SECS,
};
use argon2::{
    password_hash::{Error as PasswordHashError, SaltString},
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
};

/// 密码保护是否启用
static PASSWORD_ENABLED: AtomicBool = AtomicBool::new(false);

/// 密码的 Argon2id 哈希值（PHC 字符串格式，明文不存储）
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

/// 应用特定的 pepper 值，用于在哈希前附加到密码。
///
/// 新版在首次启动时生成随机 pepper 并持久化到配置文件（见 `PasswordConfig::pepper`），
/// 不再使用硬编码常量——硬编码 pepper 编译进二进制后对所有安装都一样，
/// 攻击者反编译即可获得，实际不提供额外防护。
const LEGACY_PEPPER: &str = "your-app-specific-pepper-change-in-production";

/// 当前生效的随机 pepper（首次加载配置时生成并持久化）
static PEPPER_KEY: LazyLock<Arc<RwLock<Option<String>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

/// 密码保护状态，返回给前端显示
#[derive(Debug, Clone, Serialize)]
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
    /// 随机 pepper（旧版本配置中不存在，缺省时回退到 LEGACY_PEPPER 验证）
    pepper: Option<String>,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            password_hash: None,
            enabled: false,
            pepper: None,
        }
    }
}

/// 单个 IP 的登录失败记录
#[derive(Debug, Clone)]
struct FailedAttempt {
    /// 连续失败次数
    count: u32,
    /// 锁定截止时间戳（0 表示未锁定）
    locked_until: i64,
    /// 最近一次失败的时间戳（用于清理过期记录）
    last_failed_at: i64,
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
///
/// 首次加载（或旧版本配置无 pepper 字段）时生成随机 pepper 并持久化。
pub fn load_password_config() {
    match config_path() {
        Ok(path) => {
            let mut config = if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<PasswordConfig>(&content) {
                        Ok(config) => config,
                        Err(e) => {
                            log::error!(
                                "[密码配置] JSON 解析失败（文件可能损坏）: {} 路径: {:?}",
                                e, path
                            );
                            PasswordConfig::default()
                        }
                    },
                    Err(e) => {
                        log::error!("[密码配置] 读取配置文件失败: {} 路径: {:?}", e, path);
                        PasswordConfig::default()
                    }
                }
            } else {
                log::info!("[密码配置] 配置文件不存在，使用默认设置: {:?}", path);
                PasswordConfig::default()
            };

            // 生成或恢复随机 pepper（旧版配置无 pepper 字段时生成并持久化）
            let pepper_missing = config.pepper.as_deref().map_or(true, |p| p.is_empty());
            if pepper_missing {
                config.pepper = Some(generate_pepper());
                log::info!("[密码配置] 已生成新的随机 pepper");
            }
            *PEPPER_KEY.write() = config.pepper.clone();

            if let Some(hash) = config.password_hash {
                let mut stored_hash = PASSWORD_HASH.write();
                *stored_hash = Some(hash);
            }
            PASSWORD_ENABLED.store(config.enabled, Ordering::SeqCst);

            // 旧配置迁移出新 pepper 后立即持久化，保证下次启动沿用同一个 pepper
            if pepper_missing {
                save_password_config();
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
        pepper: PEPPER_KEY.read().clone(),
    };
    match serde_json::to_string_pretty(&config) {
        Ok(json) => match config_path() {
            Ok(path) => match std::fs::write(&path, json) {
                Ok(_) => {
                    set_secure_file_permissions(&path);
                }
                Err(e) => log::error!("[密码配置] 保存失败: {} 路径: {:?}", e, path),
            },
            Err(e) => log::error!("[密码配置] 获取配置路径失败: {}", e),
        },
        Err(e) => log::error!("[密码配置] JSON 序列化失败: {}", e),
    }
}

/// 设置配置文件权限，尽量限制为仅当前用户可读写。
#[cfg(unix)]
fn set_secure_file_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = match std::fs::metadata(path) {
        Ok(m) => m.permissions(),
        Err(e) => {
            log::warn!("[密码配置] 读取文件元数据失败: {}", e);
            return;
        }
    };
    perms.set_mode(0o600);
    if let Err(e) = std::fs::set_permissions(path, perms) {
        log::warn!("[密码配置] 设置 Unix 文件权限失败: {}", e);
    }
}

/// 设置配置文件权限（Windows 平台）。
///
/// 注意：std::fs::Permissions 在 Windows 上仅支持只读标志，无法通过 mode 位实现
/// Unix 式的 ACL 隔离。这里仅做最佳努力设置，并在日志中说明。若需要严格的
/// 仅当前用户可读写，应额外调用 Windows API 或 PowerShell 配置 ACL。
#[cfg(windows)]
fn set_secure_file_permissions(path: &std::path::Path) {
    let mut perms = match std::fs::metadata(path) {
        Ok(m) => m.permissions(),
        Err(e) => {
            log::warn!("[密码配置] 读取文件元数据失败: {}", e);
            return;
        }
    };
    // Windows 上 set_mode 不生效于访问控制，仅保留非只读以便应用自身可写。
    perms.set_readonly(false);
    if let Err(e) = std::fs::set_permissions(path, perms) {
        log::warn!("[密码配置] 设置 Windows 文件权限失败: {}", e);
    } else {
        log::info!(
            "[密码配置] 已设置文件非只读；Windows ACL 隔离需额外配置。路径: {:?}",
            path
        );
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

/// 判断给定哈希是否为旧版 SHA-256（64 位十六进制字符串）格式
fn is_legacy_sha256_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// 获取当前使用的 pepper
///
/// 未加载配置时回退到旧版固定 pepper（测试与兼容场景），
/// 生产环境在 load_password_config 时已生成随机 pepper。
fn current_pepper() -> String {
    PEPPER_KEY
        .read()
        .clone()
        .unwrap_or_else(|| LEGACY_PEPPER.to_string())
}

/// 生成 64 字符随机十六进制 pepper
fn generate_pepper() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 使用 Argon2id 对密码进行哈希，返回 PHC 字符串格式。
/// 密码在哈希前会附加当前 pepper，salt 随机生成。
pub fn hash_password_argon2id(password: &str) -> Result<String, String> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());

    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| format!("编码 salt 失败: {}", e))?;

    let password_with_pepper = format!("{}{}", password, current_pepper());
    argon2
        .hash_password(password_with_pepper.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("Argon2id 哈希失败: {}", e))
}

/// 使用指定 pepper 验证密码（恒定时间比较由 Argon2 crate 内部保证）
fn verify_password_with_pepper(
    password: &str,
    hash_with_salt: &str,
    pepper: &str,
) -> Result<bool, String> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let parsed_hash = PasswordHash::new(hash_with_salt)
        .map_err(|e| format!("解析密码哈希失败: {}", e))?;

    let password_with_pepper = format!("{}{}", password, pepper);
    match argon2.verify_password(password_with_pepper.as_bytes(), &parsed_hash) {
        Ok(_) => Ok(true),
        Err(PasswordHashError::Password) => Ok(false),
        Err(e) => Err(format!("验证密码失败: {}", e)),
    }
}

/// 设置密码（必须为 4 位纯数字），使用 Argon2id 哈希后存储
pub fn set_password(password: &str) -> Result<(), String> {
    if password.len() != 4 {
        return Err("密码必须是4位数字".to_string());
    }
    if !password.chars().all(|c| c.is_ascii_digit()) {
        return Err("密码只能包含数字0-9".to_string());
    }

    let hash = hash_password_argon2id(password)?;

    {
        let mut stored_hash = PASSWORD_HASH.write();
        *stored_hash = Some(hash);
    }
    // 密码已变更，清除所有已登录 session，使旧会话立即失效
    SESSIONS.write().clear();
    save_password_config();
    Ok(())
}

/// 验证密码是否正确（与存储的哈希值比对）
pub fn verify_password(password: &str) -> Result<bool, String> {
    let hash_guard = PASSWORD_HASH.read();
    let Some(hash) = hash_guard.as_ref() else {
        return Err("未设置密码".to_string());
    };

    if is_legacy_sha256_hash(hash) {
        log::warn!("[密码配置] 检测到旧版 SHA-256 哈希，视为未验证通过，请重新设置密码");
        return Ok(false);
    }

    let pepper = current_pepper();
    if verify_password_with_pepper(password, hash, &pepper)? {
        return Ok(true);
    }

    // 旧版本使用固定 pepper 生成的哈希：用 legacy pepper 再验证一次，
    // 成功后自动使用当前随机 pepper 重哈希并迁移配置，用户无感知
    if pepper != LEGACY_PEPPER && verify_password_with_pepper(password, hash, LEGACY_PEPPER)? {
        log::info!("[密码配置] 旧版 pepper 哈希验证通过，正在迁移到新 pepper");
        drop(hash_guard);
        if let Ok(new_hash) = hash_password_argon2id(password) {
            *PASSWORD_HASH.write() = Some(new_hash);
            save_password_config();
        }
        return Ok(true);
    }

    Ok(false)
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

/// 启动后台线程，定期清理过期 session
pub fn start_cleanup_thread() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(SESSION_CLEANUP_INTERVAL_SECS));
        cleanup_expired_sessions();
    });
}

/// 检查指定 IP 是否被频率限制锁定
///
/// 连续失败 MAX_FAILED_ATTEMPTS 次后，IP 将被锁定 LOCK_DURATION_SECS 秒
pub fn check_rate_limit(ip: &str) -> Result<(), String> {
    let mut attempts = FAILED_ATTEMPTS.write();
    prune_expired_attempts(&mut attempts);
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

/// 清理已过期的失败记录，防止 FAILED_ATTEMPTS 无限增长
///
/// 过期标准：锁定已结束（或从未锁定）且最近一次失败超出 LOCK_DURATION_SECS 窗口。
/// 仍在锁定中、或仍在窗口内累计失败次数的记录保留。
fn prune_expired_attempts(attempts: &mut HashMap<String, FailedAttempt>) {
    let now = current_timestamp();
    attempts.retain(|_, a| {
        if a.locked_until > now {
            return true;
        }
        now - a.last_failed_at < LOCK_DURATION_SECS
    });
}

/// 记录一次登录失败，达到阈值后锁定该 IP
pub fn record_failed_attempt(ip: &str) {
    let mut attempts = FAILED_ATTEMPTS.write();
    prune_expired_attempts(&mut attempts);
    let now = current_timestamp();

    let attempt = attempts.entry(ip.to_string()).or_insert(FailedAttempt {
        count: 0,
        locked_until: 0,
        last_failed_at: now,
    });

    attempt.count += 1;
    attempt.last_failed_at = now;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 涉及全局 pepper/哈希状态的测试串行执行，避免并行交叉污染
    static PASSWORD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_hash_password_argon2id() {
        let _guard = PASSWORD_TEST_LOCK.lock().unwrap();
        let hash = hash_password_argon2id("1234").expect("哈希应成功");
        assert!(
            hash.starts_with("$argon2id$"),
            "PHC 字符串应以 $argon2id$ 开头"
        );
    }

    #[test]
    fn test_verify_password_argon2id_correct() {
        let _guard = PASSWORD_TEST_LOCK.lock().unwrap();
        let password = "5678";
        let hash = hash_password_argon2id(password).expect("哈希应成功");
        let result = verify_password_with_pepper(password, &hash, &current_pepper())
            .expect("验证不应报错");
        assert!(result, "正确密码应验证通过");
    }

    #[test]
    fn test_verify_password_argon2id_wrong() {
        let _guard = PASSWORD_TEST_LOCK.lock().unwrap();
        let hash = hash_password_argon2id("0000").expect("哈希应成功");
        let result = verify_password_with_pepper("9999", &hash, &current_pepper())
            .expect("验证不应报错");
        assert!(!result, "错误密码应验证失败");
    }

    #[test]
    fn test_legacy_sha256_recognized() {
        let sha256_hash = "a".repeat(64);
        assert!(
            is_legacy_sha256_hash(&sha256_hash),
            "64 位十六进制字符串应被识别为旧版 SHA-256 哈希"
        );

        let phc_hash = "$argon2id$v=19$m=19456,t=2,p=1$...$...";
        assert!(
            !is_legacy_sha256_hash(phc_hash),
            "PHC 字符串不应被识别为旧版 SHA-256 哈希"
        );
    }

    #[test]
    fn test_set_password_validation() {
        let _guard = PASSWORD_TEST_LOCK.lock().unwrap();
        assert!(
            set_password("123").is_err(),
            "少于 4 位的密码应校验失败"
        );
        assert!(
            set_password("12345").is_err(),
            "多于 4 位的密码应校验失败"
        );
        assert!(
            set_password("abcd").is_err(),
            "非数字密码应校验失败"
        );
        assert!(
            set_password("12a4").is_err(),
            "包含非数字字符的密码应校验失败"
        );
    }

    #[test]
    fn test_verify_password_migrates_legacy_pepper() {
        let _guard = PASSWORD_TEST_LOCK.lock().unwrap();
        // 模拟旧版配置：使用固定 LEGACY_PEPPER 生成哈希
        let legacy_hash = {
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
            let mut salt_bytes = [0u8; 16];
            rand::rng().fill_bytes(&mut salt_bytes);
            let salt = SaltString::encode_b64(&salt_bytes).expect("编码 salt 应成功");
            let password_with_pepper = format!("{}{}", "1234", LEGACY_PEPPER);
            argon2
                .hash_password(password_with_pepper.as_bytes(), &salt)
                .expect("哈希应成功")
                .to_string()
        };

        // 模拟生产环境：当前 pepper 已切换为随机值
        *PEPPER_KEY.write() = Some("a".repeat(64));
        *PASSWORD_HASH.write() = Some(legacy_hash);

        // 旧版 pepper 的密码应验证通过，并自动迁移到新 pepper
        assert!(
            verify_password("1234").expect("验证不应报错"),
            "旧版 pepper 的密码应验证通过"
        );
        let migrated = PASSWORD_HASH.read().clone().expect("迁移后应存在哈希");
        assert!(
            verify_password_with_pepper("1234", &migrated, &current_pepper())
                .expect("验证不应报错"),
            "迁移后哈希应使用当前 pepper"
        );

        // 错误密码不应触发迁移
        *PASSWORD_HASH.write() = Some(migrated);
        assert!(
            !verify_password("9999").expect("验证不应报错"),
            "错误密码应验证失败"
        );

        // 清理全局状态，避免影响其他测试
        *PASSWORD_HASH.write() = None;
        *PEPPER_KEY.write() = None;
    }
}
