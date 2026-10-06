//! HTTP 请求路由与处理
//!
//! 根据请求 URL 和方法分发到对应的处理逻辑：
//! - `POST /auth`: 密码认证
//! - `GET /`: SvelteKit 构建产物（桌面端与网页端共用的同一个前端）
//! - `GET /videos`: 视频列表 JSON API
//! - `GET /refresh`: 触发视频重新扫描
//! - `GET /refresh-status`: 查询刷新结果
//! - `GET /video/*`: 视频文件流式服务
//! - `GET /_app/*` 等: 前端静态资源（JS/CSS/图片）
//! - 其他: 404
//!
//! 密码保护启用时，未认证请求会被重定向到登录页。
//!
//! 前端只有一份：网页端加载的正是 Tauri 内嵌的 `frontendDist`
//! （见 [`super::assets`]），因此网页端与桌面端不存在第二套 UI 实现。

use std::io::Read;

use crate::constants::REFRESH_COOLDOWN_SECS;
use crate::models::VideoSummary;
use crate::AppState;

/// 登录页面 HTML 模板（编译时嵌入）
static LOGIN_PAGE_TEMPLATE: &str = include_str!("../login_template.html");

/// 设计令牌（编译时嵌入同一份 `theme.css`）
///
/// 桌面端组件与登录页共用这一个来源，避免第二份 `:root` 变量随主题演进而漂移。
static THEME_TOKENS: &str = include_str!("../../../src/lib/styles/theme.css");

/// 内容哈希命名的构建产物，可长期强缓存
const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// 构建产物目录下的非哈希文件（每次构建都可能变化），禁止缓存
const CACHE_NO_STORE: &str = "no-store";

/// 其他静态资源（favicon 等）
const CACHE_SHORT: &str = "public, max-age=3600";

/// 按名称读取请求头值，字段名大小写不敏感（HTTP 头名规范要求）
///
/// 找不到时返回 `None`；调用方需要默认空串时可自行 `unwrap_or("")`。
fn request_header<'a>(request: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

/// 从 Host 头中取出主机名，丢弃端口部分
///
/// IPv6 字面量形如 `[::1]:6008`，方括号内本身含冒号，不能直接按 `:` 切分
/// （否则会得到 `[`，导致 IPv6 访问被误判为非法 Host）。
fn host_without_port(header: &str) -> &str {
    let host = header.trim();
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    host.split(':').next().unwrap_or(host)
}

/// 判断请求的 Host 头是否为本机地址
///
/// 防 DNS rebinding：恶意网页可以把域名解析到本机，但无法让浏览器把 Host
/// 伪装成本机 IP / localhost，因此拒绝其他 Host 即可阻断"同源"读取。
fn is_host_allowed(host_header: Option<&str>, ips: &[String]) -> bool {
    host_header
        .map(host_without_port)
        .map(|host| {
            ips.iter().any(|ip| ip == host)
                || host == "127.0.0.1"
                || host == "localhost"
                || host == "::1"
        })
        .unwrap_or(false)
}

/// 渲染登录页：注入设计令牌
///
/// 令牌来自与桌面端共享的 `theme.css`，因此登录页不再是第二份主题定义的持有者。
fn render_login_page() -> String {
    LOGIN_PAGE_TEMPLATE.replace("/* {theme_tokens} */", THEME_TOKENS)
}

/// 返回前端静态资源响应；未配置资源来源或未命中时返回 `None`
///
/// 缓存策略按 SvelteKit 产物约定区分：`_app/immutable/` 下是内容哈希命名的文件，
/// 可强缓存；`_app/env.js`、`_app/version.json` 每次构建都会变，必须禁止缓存。
///
/// CSP 由本函数自行生成（注入 nonce），而不是采用 `Asset::csp_header`：
/// 桌面端 webview 的 CSP 由 `tauri-codegen` 在构建期为内联脚本预计算 sha256 哈希后
/// 拼出，只适用于 Tauri 自己的资源协议；浏览器这一侧无法复用那份头部，
/// 因此改为每次请求生成随机 nonce。两者同样不放行 `script-src 'unsafe-inline'`。
fn serve_frontend_asset(
    app_state: &AppState,
    url: &str,
) -> Option<tiny_http::Response<Box<dyn Read + Send>>> {
    let assets = app_state.frontend_assets()?;
    let asset = assets.get(url)?;

    let is_html = asset.mime_type.starts_with("text/html");
    if is_html {
        let nonce = super::response::generate_nonce();
        let html = String::from_utf8_lossy(&asset.bytes).into_owned();
        return Some(super::response::frontend_html_response(&html, &nonce));
    }

    let cache_control = if url.starts_with("/_app/immutable/") {
        CACHE_IMMUTABLE
    } else if url.starts_with("/_app/") {
        CACHE_NO_STORE
    } else {
        CACHE_SHORT
    };
    Some(super::response::static_asset_response(
        asset.bytes,
        &asset.mime_type,
        cache_control,
    ))
}

/// HTTP 请求主处理函数
///
/// 根据请求 URL 和方法路由到对应的处理逻辑。
/// 密码保护启用时，未认证请求会被重定向到 `/login`。
pub fn handle_request(
    request: &mut tiny_http::Request,
    ips: &[String],
    app_state: &AppState,
) -> tiny_http::Response<Box<dyn Read + Send>> {
    // Host 头校验，防止 DNS rebinding 攻击：
    // 仅允许本机 IP（含回环地址）作为 Host，恶意网页无法通过域名解析指向本机后"同源"读取 /videos。
    if !is_host_allowed(request_header(request, "Host"), ips) {
        return super::response::text_response(403, "Invalid Host header");
    }

    let url = request.url().to_string();
    let method = request.method().clone();

    if url == "/auth" && method == tiny_http::Method::Post {
        return super::auth::handle_auth(request, app_state);
    }

    let password_enabled = app_state.password().is_enabled();
    let authenticated = if password_enabled {
        app_state
            .password()
            .check_web_auth(request_header(request, "Cookie").unwrap_or(""))
    } else {
        true
    };

    // 登录页自身必须在未认证时可达，其余（含前端静态资源）一律先认证
    if url == "/login" || url == "/login.html" {
        if authenticated {
            return super::response::redirect_response("/");
        }
        let nonce = super::response::generate_nonce();
        return super::response::frontend_html_response(&render_login_page(), &nonce);
    }

    if !authenticated {
        return super::response::redirect_response("/login");
    }

    match url.as_str() {
        "/" | "/index.html" => match serve_frontend_asset(app_state, "/") {
            Some(resp) => resp,
            None => {
                log::warn!("[HTTP服务器] 前端资源不可用，无法提供首页");
                super::response::text_response(
                    503,
                    "前端资源不可用：请先执行 pnpm build 构建前端产物",
                )
            }
        },
        "/videos" => {
            let etag = format!("\"{}\"", app_state.videos_etag());

            let if_none_match = request_header(request, "If-None-Match");

            if if_none_match == Some(etag.as_str()) {
                let mut resp = super::response::text_response(304, "");
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"ETag"[..], etag.as_bytes()).unwrap(),
                );
                resp.add_header(
                    tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"must-revalidate"[..])
                        .unwrap(),
                );
                return resp;
            }

            let videos = app_state.shared_videos();
            // 只返回摘要：VideoFile.path 是本机绝对路径，不应泄露给局域网客户端
            let summaries: Vec<VideoSummary> = videos.iter().map(VideoSummary::from).collect();
            let json = serde_json::to_string(&summaries).unwrap_or_else(|_| "[]".to_string());
            let mut resp = super::response::json_response(200, &json);
            resp.add_header(tiny_http::Header::from_bytes(&b"ETag"[..], etag.as_bytes()).unwrap());
            resp.add_header(
                tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"must-revalidate"[..])
                    .unwrap(),
            );
            resp
        }
        "/refresh" => {
            if !app_state.start_refresh() {
                let json = serde_json::json!({"success": false, "message": "正在刷新中，请稍后"})
                    .to_string();
                return super::response::json_response(429, &json);
            }
            if !app_state.start_refresh_cooldown() {
                app_state.finish_refresh();
                let json =
                    serde_json::json!({"success": false, "message": "刷新过于频繁，请稍后再试"})
                        .to_string();
                return super::response::json_response(429, &json);
            }

            let folder_path = app_state.shared_folder_path();
            if folder_path.is_empty() {
                app_state.finish_refresh();
                app_state.finish_refresh_cooldown();
                let json = serde_json::json!({"success": false, "message": "未设置共享文件夹"})
                    .to_string();
                return super::response::json_response(400, &json);
            }

            // 清空上次刷新结果，使 /refresh-status 能准确反映本次刷新状态
            app_state.clear_refresh_result();

            let scan_app_state = app_state.clone();
            std::thread::spawn(move || {
                // 扫描路径不使用 unwrap / 索引，也没有 panic 源；且 release 构建为
                // panic = "abort"，catch_unwind 在此场景下本就无法生效，故不做包装
                let msg =
                    match crate::commands::video::scan_videos_sync(folder_path, &scan_app_state) {
                        Ok(report) => {
                            scan_app_state.set_last_scan_report(report.clone());
                            // 提示语里带上"被跳过的文件"，让网页端用户也能知道
                            // 列表为什么比目录里的文件少
                            let mut json = serde_json::json!({
                                "success": true,
                                "message": "视频列表已刷新",
                                "total": report.total,
                            });
                            if report.has_skipped() {
                                json["skipped_small_count"] =
                                    serde_json::json!(report.skipped_small_count);
                                json["message"] = serde_json::json!(format!(
                                    "视频列表已刷新；{} 个文件因小于最小体积被跳过",
                                    report.skipped_small_count
                                ));
                            }
                            json.to_string()
                        }
                        Err(e) => serde_json::json!({"success": false, "message": e.to_string()})
                            .to_string(),
                    };
                log::info!("[刷新] {}", msg);
                scan_app_state.set_refresh_result(msg);
                scan_app_state.finish_refresh();
            });

            let cooldown_app_state = app_state.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(REFRESH_COOLDOWN_SECS));
                cooldown_app_state.finish_refresh_cooldown();
            });

            let json = serde_json::json!({"success": true, "message": "刷新已开始"}).to_string();
            super::response::json_response(202, &json)
        }
        "/refresh-status" => {
            let result = app_state.refresh_result();
            match result {
                Some(msg) => super::response::json_response(200, &msg),
                // pending 字段是前端判断"是否已有结果"的依据，
                // 避免前后端靠 message 文案耦合（改文案会静默改变轮询行为）
                None => super::response::json_response(
                    200,
                    r#"{"success": true, "pending": true, "message": "无刷新记录"}"#,
                ),
            }
        }
        _ if url.starts_with("/video/") => {
            let range_header = request_header(request, "Range");
            super::video_serve::handle_video_request(&url, range_header, app_state)
        }
        _ => match serve_frontend_asset(app_state, &url) {
            Some(resp) => resp,
            None => super::response::text_response(404, "Not found"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_without_port_strips_port() {
        assert_eq!(host_without_port("192.168.1.5:6008"), "192.168.1.5");
        assert_eq!(host_without_port("localhost:6008"), "localhost");
        assert_eq!(host_without_port("localhost"), "localhost");
        assert_eq!(host_without_port(" 192.168.1.5:6008 "), "192.168.1.5");
    }

    #[test]
    fn test_host_without_port_handles_ipv6_literal() {
        assert_eq!(host_without_port("[::1]:6008"), "::1");
        assert_eq!(host_without_port("[::1]"), "::1");
        assert_eq!(host_without_port("[fe80::1]:6008"), "fe80::1");
    }

    #[test]
    fn test_is_host_allowed() {
        let ips = vec!["192.168.1.5".to_string()];

        assert!(is_host_allowed(Some("192.168.1.5:6008"), &ips));
        assert!(is_host_allowed(Some("localhost:6008"), &ips));
        assert!(is_host_allowed(Some("127.0.0.1"), &ips));
        assert!(is_host_allowed(Some("[::1]:6008"), &ips));

        // DNS rebinding：域名解析到本机，但 Host 仍是域名
        assert!(!is_host_allowed(Some("evil.example.com"), &ips));
        assert!(!is_host_allowed(Some("192.168.1.6:6008"), &ips));
        assert!(!is_host_allowed(None, &ips));
    }

    #[test]
    fn test_render_login_page_injects_theme_tokens() {
        let html = render_login_page();
        assert!(
            !html.contains("{theme_tokens}"),
            "占位符应已被 theme.css 替换"
        );
        assert_eq!(
            html.matches(":root {").count(),
            1,
            "登录页应只有 theme.css 注入的那一份 :root 定义"
        );
        // 断言只有 theme.css 才有的令牌：否则"登录页自带的旧 :root 恰好也定义了
        // --bg / --accent"会让测试在令牌没有真正注入时照样通过
        assert!(
            html.contains("--surface-raised:"),
            "登录页应注入 theme.css 的完整令牌集"
        );
        assert!(html.contains("--warning:"), "登录页应包含 --warning 令牌");
        assert!(
            html.contains("--radius-lg:"),
            "登录页应包含 theme.css 的圆角令牌"
        );
        // 模板里用到的每个令牌都必须在注入的 theme.css 中真实存在，
        // 否则属性会静默失效（例如遮罩变透明）
        for token in [
            "--scrim:",
            "--scrim-strong:",
            "--shadow-card:",
            "--warning-soft:",
            "--warning-border:",
            "--success-light:",
            "--success-border:",
            "--surface-hover:",
            "--border-strong:",
            "--accent-soft:",
            "--accent-border:",
            "--accent-soft-hover:",
            "--danger-soft:",
            "--danger-border:",
            "--danger-strong:",
        ] {
            assert!(
                html.contains(token),
                "登录页注入的 theme.css 缺少模板使用的 {} 令牌",
                token
            );
        }
    }

    /// 检测 CSS 颜色字面量（`#rgb` / `#rrggbb` 与 `rgb(` / `rgba(`）
    fn contains_color_literal(source: &str) -> bool {
        if source.contains("rgb(") || source.contains("rgba(") {
            return true;
        }
        let bytes = source.as_bytes();
        bytes.iter().enumerate().any(|(i, b)| {
            *b == b'#'
                && bytes
                    .get(i + 1)
                    .is_some_and(|next| next.is_ascii_hexdigit())
        })
    }

    #[test]
    fn test_login_template_has_no_hardcoded_colors() {
        // 登录页的颜色必须全部来自注入的 theme.css：模板里一旦出现颜色字面量，
        // 改主题时就会漏掉这一处（AGENTS.md「禁止硬编码颜色」）
        assert!(
            !contains_color_literal(LOGIN_PAGE_TEMPLATE),
            "登录页模板不应硬编码颜色，请改用 theme.css 的令牌"
        );
    }
}
