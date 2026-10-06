//! 内嵌 HTTP 服务器的端到端测试
//!
//! 直接启动真实的 `tiny_http` 服务器并用裸 TCP 发请求，覆盖此前完全没有测试的
//! 请求编排层：Host 校验、认证门、SPA / 静态资源路由与缓存策略、CSP nonce 注入、
//! `/videos` 的 ETag 与"不泄露绝对路径"约束、`/video/*` 的扩展名与穿越防护。
//!
//! 前端资源使用 [`FrontendAssets::Directory`] 指向临时目录，因此本文件不依赖
//! "先构建前端"这一前置步骤，`cargo test` 可独立运行。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::assets::FrontendAssets;
use super::{start_http_server, RunningServer, StopSignal};
use crate::test_utils::{
    create_test_video, http_request, http_request_with_body, join_all_with_timeout,
    make_frontend_dist, make_temp_dir, sample_video,
};
use crate::AppState;

/// 一个跑在临时端口上的测试服务器，`Drop` 时自动停机并清理临时目录
struct TestServer {
    app_state: Arc<AppState>,
    running: RunningServer,
    handles: Vec<std::thread::JoinHandle<()>>,
    port: u16,
    frontend_dir: PathBuf,
    password_dir: PathBuf,
}

impl TestServer {
    /// 启动服务器（前端资源指向磁盘 fixture，状态停在 `Starting`，与生产启动窗口一致）
    fn start(configure: impl FnOnce(&AppState)) -> Self {
        let frontend_dir = make_frontend_dist("http_api_frontend");
        let password_dir = make_temp_dir("http_api_password");

        let app_state = Arc::new(AppState::new());
        app_state.set_frontend_assets(FrontendAssets::directory(frontend_dir.clone()));
        // 避免密码配置写到测试可执行文件所在目录
        app_state.password().set_config_dir(password_dir.clone());
        configure(&app_state);

        let (running, handles) = start_http_server(
            &["127.0.0.1".to_string()],
            0,
            app_state.clone(),
            StopSignal::new(),
        )
        .expect("测试服务器应能启动");
        let port = running
            .server()
            .server_addr()
            .to_ip()
            .expect("应绑定到 IP 地址")
            .port();

        Self {
            app_state,
            running,
            handles,
            port,
            frontend_dir,
            password_dir,
        }
    }

    fn get(&self, path: &str) -> crate::test_utils::HttpResponse {
        http_request(self.port, "GET", path, &[])
    }

    fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> crate::test_utils::HttpResponse {
        http_request(self.port, "GET", path, headers)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let handles = std::mem::take(&mut self.handles);
        self.running.shutdown(handles.len());
        let _ = join_all_with_timeout(handles, Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(&self.frontend_dir);
        let _ = std::fs::remove_dir_all(&self.password_dir);
    }
}

// ---------------- Host 校验 ----------------

#[test]
fn rejects_request_with_foreign_host_header() {
    let server = TestServer::start(|_| {});

    let resp = server.get_with("/videos", &[("Host", "evil.example.com")]);
    assert_eq!(
        resp.status, 403,
        "非本机 Host 必须被拒绝（防 DNS rebinding）"
    );

    let resp = server.get_with("/videos", &[("Host", "127.0.0.1:1")]);
    assert_eq!(resp.status, 200, "本机 IP 加端口应被放行");
}

// ---------------- SPA 与静态资源 ----------------

#[test]
fn serves_spa_index_with_nonce_csp() {
    let server = TestServer::start(|_| {});
    let resp = server.get("/");

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.header("Content-Type"),
        Some("text/html; charset=utf-8"),
        "首页必须是 HTML"
    );
    assert_eq!(
        resp.header("Cache-Control"),
        Some("no-store"),
        "首页携带 nonce，禁止缓存"
    );

    // CSP 中的 nonce 必须与页面里注入了 nonce 的脚本一致，否则脚本会被浏览器拦下
    let csp = resp.header("Content-Security-Policy").expect("应设置 CSP");
    let nonce = csp
        .split("'nonce-")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .expect("CSP 应包含 nonce");
    assert!(!nonce.is_empty(), "nonce 不应为空");
    assert!(
        resp.body.contains(&format!("<script nonce=\"{}\">", nonce)),
        "内联脚本必须注入与 CSP 一致的 nonce，实际正文: {}",
        resp.body
    );
    assert!(
        csp.contains("script-src 'self' 'nonce-"),
        "script-src 只应放行 self 与该 nonce，不应放行 unsafe-inline：{}",
        csp
    );
}

#[test]
fn serves_immutable_assets_with_long_cache() {
    let server = TestServer::start(|_| {});
    let resp = server.get("/_app/immutable/entry/app.abc123.js");

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.header("Content-Type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        resp.header("Cache-Control"),
        Some("public, max-age=31536000, immutable"),
        "内容哈希命名的产物应可长期强缓存"
    );
}

#[test]
fn serves_non_hashed_assets_without_cache() {
    let server = TestServer::start(|_| {});
    let resp = server.get("/_app/env.js");

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.header("Cache-Control"),
        Some("no-store"),
        "每次构建都会变的文件不应被缓存"
    );
}

#[test]
fn serves_favicon_and_rejects_unknown_and_traversal_paths() {
    let server = TestServer::start(|_| {});

    let favicon = server.get("/favicon.png");
    assert_eq!(favicon.status, 200);
    assert_eq!(favicon.header("Content-Type"), Some("image/png"));

    assert_eq!(server.get("/no-such-file.js").status, 404);
    assert_eq!(
        server.get("/../Cargo.toml").status,
        404,
        "路径穿越不得读到资源根之外的文件"
    );
    assert_eq!(server.get("/%2e%2e/Cargo.toml").status, 404);
}

#[test]
fn returns_503_when_frontend_assets_are_unavailable() {
    // 不注入前端资源，模拟构建产物缺失
    let app_state = Arc::new(AppState::new());
    let (running, handles) =
        start_http_server(&["127.0.0.1".to_string()], 0, app_state, StopSignal::new())
            .expect("测试服务器应能启动");
    let port = running.server().server_addr().to_ip().unwrap().port();

    let resp = http_request(port, "GET", "/", &[]);
    assert_eq!(resp.status, 503, "缺少前端产物时应给出明确的 503");

    running.shutdown(handles.len());
    let _ = join_all_with_timeout(handles, Duration::from_secs(5));
}

// ---------------- /videos ----------------

#[test]
fn videos_endpoint_omits_absolute_path_and_supports_etag() {
    let server = TestServer::start(|state| {
        state.set_shared_videos(vec![
            sample_video("movies/a.mp4", 1024, Some("2024-01-01 10:00:00")),
            sample_video("b.mp4", 2048, None),
        ]);
    });

    let resp = server.get("/videos");
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("movies/a.mp4"), "应包含相对路径");
    assert!(
        !resp.body.contains("\"path\":"),
        "响应不得包含本机绝对路径字段 path，实际正文: {}",
        resp.body
    );
    assert!(
        !resp.body.contains(r"C:\videos"),
        "响应不得泄露本机绝对路径，实际正文: {}",
        resp.body
    );
    assert!(
        resp.body.contains("\"relative_path\":"),
        "应保留客户端播放所需的 relative_path，实际正文: {}",
        resp.body
    );

    let etag = resp.header("ETag").expect("应返回 ETag").to_string();
    let cached = server.get_with("/videos", &[("If-None-Match", &etag)]);
    assert_eq!(cached.status, 304, "指纹未变化时应返回 304");
    assert!(cached.body.is_empty(), "304 不应包含正文");
}

// ---------------- 认证门 ----------------

#[test]
fn login_gate_redirects_protected_paths_and_serves_login_page() {
    let server = TestServer::start(|state| {
        state
            .password()
            .set_password("1234")
            .expect("设置密码应成功");
        state.password().set_enabled(true);
    });

    for path in ["/", "/videos", "/_app/env.js", "/video/a.mp4"] {
        let resp = server.get(path);
        assert_eq!(resp.status, 302, "未认证访问 {} 应跳转登录页", path);
        assert_eq!(resp.header("Location"), Some("/login"));
    }

    let login = server.get("/login");
    assert_eq!(login.status, 200);
    assert!(
        login.body.contains("--accent: #3b82f6"),
        "登录页应使用与桌面端共享的 theme.css 令牌"
    );
}

#[test]
fn auth_endpoint_issues_cookie_and_unlocks_protected_paths() {
    let server = TestServer::start(|state| {
        state
            .password()
            .set_password("1234")
            .expect("设置密码应成功");
        state.password().set_enabled(true);
    });

    // 错误密码：401，且必须记录失败次数
    let wrong = http_request_with_body(
        server.port,
        "POST",
        "/auth",
        &[],
        Some(r#"{"password":"9999"}"#),
    );
    assert_eq!(wrong.status, 401);

    // 正确密码：200，下发 HttpOnly + SameSite=Strict 的 session cookie
    let ok = http_request_with_body(
        server.port,
        "POST",
        "/auth",
        &[],
        Some(r#"{"password":"1234"}"#),
    );
    assert_eq!(ok.status, 200);
    let cookie = ok.header("Set-Cookie").expect("应下发 cookie");
    assert!(
        cookie.contains("HttpOnly"),
        "cookie 应为 HttpOnly：{}",
        cookie
    );
    assert!(
        cookie.contains("SameSite=Strict"),
        "cookie 应为 SameSite=Strict：{}",
        cookie
    );
    let token = cookie
        .split("session_token=")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .expect("cookie 应包含 session_token");

    // 携带会话后受保护路径应放行
    let cookie_header = format!("session_token={}", token);
    let resp = server.get_with("/videos", &[("Cookie", &cookie_header)]);
    assert_eq!(resp.status, 200, "携带有效会话后应放行");

    // 已认证访问 /login 应跳回首页
    let login = server.get_with("/login", &[("Cookie", &cookie_header)]);
    assert_eq!(login.status, 302);
    assert_eq!(login.header("Location"), Some("/"));
}

#[test]
fn login_redirects_to_root_when_password_protection_is_disabled() {
    let server = TestServer::start(|_| {});
    let resp = server.get("/login");
    assert_eq!(resp.status, 302, "未启用密码保护时登录页无意义");
    assert_eq!(resp.header("Location"), Some("/"));
}

// ---------------- /video/* ----------------

#[test]
fn video_endpoint_serves_videos_and_rejects_other_files() {
    let media_dir = make_temp_dir("http_api_media");
    create_test_video(&media_dir.join("movie.mp4"), 4096);
    std::fs::write(media_dir.join("notes.txt"), b"top secret").expect("写入 txt 失败");
    std::fs::write(media_dir.join("db.sqlite"), b"secret").expect("写入 db 失败");

    let server = TestServer::start(|state| {
        state.set_shared_folder_path(media_dir.to_string_lossy().to_string());
    });

    let movie = server.get("/video/movie.mp4");
    assert_eq!(movie.status, 200, "受支持的视频扩展名应可播放");
    assert_eq!(movie.header("Content-Type"), Some("video/mp4"));

    for path in ["/video/notes.txt", "/video/db.sqlite"] {
        let resp = server.get(path);
        assert_eq!(
            resp.status, 403,
            "{} 不是视频文件，必须被拒绝（否则共享目录变成任意文件下载端点）",
            path
        );
    }

    assert_eq!(
        server.get("/video/..%2f..%2fCargo.toml").status,
        403,
        "路径穿越必须被拒绝"
    );
    assert_eq!(server.get("/video/").status, 400, "空视频路径应返回 400");

    let _ = std::fs::remove_dir_all(&media_dir);
}

#[test]
fn app_state_exposes_shared_videos_after_scan_style_update() {
    // 保证 /videos 走的是 AppState 中当前列表（Arc 指针替换后 ETag 缓存随之失效）
    let server = TestServer::start(|state| {
        state.set_shared_videos(vec![sample_video("a.mp4", 1024, None)]);
    });
    let first = server.get("/videos");
    let first_etag = first.header("ETag").expect("应有 ETag").to_string();

    server.app_state.set_shared_videos(vec![
        sample_video("a.mp4", 1024, None),
        sample_video("b.mp4", 1, None),
    ]);

    let second = server.get("/videos");
    assert_eq!(second.status, 200);
    assert_ne!(
        second.header("ETag"),
        Some(first_etag.as_str()),
        "列表变化后 ETag 必须变化，否则网页端会一直显示陈旧列表"
    );
    assert!(second.body.contains("b.mp4"));
}

// ---------------- 真实构建产物 ----------------

/// 从 HTML 中提取所有以 `/` 开头的 `href` / `src` 引用
fn referenced_asset_paths(html: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for prefix in ["href=\"", "src=\""] {
        let mut rest = html;
        while let Some(found) = rest.find(prefix) {
            let after = &rest[found + prefix.len()..];
            if let Some(value) = after.strip_prefix('/') {
                if let Some(end) = value.find('"') {
                    paths.push(format!("/{}", &value[..end]));
                }
            }
            rest = after;
        }
    }
    paths
}

/// 真实前端构建产物的端到端校验
///
/// 依赖仓库根目录的 `build/`（由 `pnpm build` 生成）。默认运行 `cargo test` 时
/// 标记为 `#[ignore]`——"必须先构建前端"不应成为跑 Rust 单测的前置条件；
/// CI 在 `pnpm build` 之后显式执行 `cargo test -- --ignored` 来覆盖它，
/// 因此它是被显式跳过的测试，而不是静默通过。
///
/// 与 fixture 版测试的区别：这里用的是 SvelteKit 真实产物，能发现
/// "路由逻辑正确、但真实产物形状对不上"的问题（nonce 注入、资源引用、缓存策略）。
#[test]
#[ignore = "需要先执行 pnpm build 生成 build/ 产物"]
fn real_build_output_is_served_end_to_end() {
    let dist = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../build");
    assert!(
        dist.join("index.html").exists(),
        "缺少 {}/index.html，请先在仓库根目录执行 pnpm build",
        dist.display()
    );

    let app_state = Arc::new(AppState::new());
    app_state.set_frontend_assets(FrontendAssets::directory(dist.clone()));
    let (running, handles) = start_http_server(
        &["127.0.0.1".to_string()],
        0,
        app_state.clone(),
        StopSignal::new(),
    )
    .expect("测试服务器应能启动");
    let port = running.server().server_addr().to_ip().unwrap().port();

    let index = http_request(port, "GET", "/", &[]);
    assert_eq!(index.status, 200, "真实产物必须能作为首页提供");

    // CSP nonce 必须与页面里注入的一致，且脚本不放行 unsafe-inline
    let csp = index.header("Content-Security-Policy").expect("应设置 CSP");
    assert!(
        csp.contains("script-src 'self' 'nonce-"),
        "script-src 应只放行 self 与 nonce：{}",
        csp
    );
    let nonce = csp
        .split("'nonce-")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .expect("CSP 应包含 nonce");
    assert!(
        index
            .body
            .contains(&format!("<script nonce=\"{}\">", nonce)),
        "真实 index.html 的内联脚本必须被注入 nonce"
    );

    // 产物引用的每个资源都必须能取到
    let paths = referenced_asset_paths(&index.body);
    assert!(!paths.is_empty(), "真实产物应引用至少一个资源");
    for path in &paths {
        let resp = http_request(port, "GET", path, &[]);
        assert_eq!(resp.status, 200, "产物引用的资源应可访问: {}", path);
    }

    // 内容哈希命名的 chunk 必须走强缓存
    let chunk = paths
        .iter()
        .find(|p| p.starts_with("/_app/immutable/"))
        .expect("真实产物应包含 _app/immutable 下的资源");
    let chunk_resp = http_request(port, "GET", chunk, &[]);
    assert_eq!(chunk_resp.status, 200);
    assert_eq!(
        chunk_resp.header("Cache-Control"),
        Some("public, max-age=31536000, immutable"),
        "内容哈希资源应可长期强缓存"
    );

    running.shutdown(handles.len());
    let _ = join_all_with_timeout(handles, Duration::from_secs(5));
}
