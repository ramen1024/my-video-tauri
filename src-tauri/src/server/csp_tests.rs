//! 跨文件一致性测试
//!
//! 项目里有几处"同一个决策写在两个地方"的配置，它们的重复本身无法消除（一份给
//! `tauri-codegen`，一份给内嵌 HTTP 服务器；一份给 Rust，一份给 TypeScript），
//! 但**漂移**必须能在 `cargo test` 里被立刻发现。本文件专门放这类断言。
//!
//! 之所以单独成文件而不是塞进 `api_tests`：`api_tests` 要起真实 HTTP 服务器、
//! 依赖裸 TCP，本文件只做纯解析/字符串比对，快得多，也更容易定位失败原因。

use crate::constants::{
    CSP_SHARED_DIRECTIVES, DEFAULT_SHARE_PORT, DEV_HMR_PORT, DEV_SERVER_PORT,
    MIN_VIDEO_FILE_SIZE_BYTES, WEB_CSP_HARDENED_DIRECTIVES,
};

/// 把 CSP 字符串解析成 `指令 -> 取值` 列表
fn parse_csp(csp: &str) -> Vec<(String, String)> {
    csp.split(';')
        .filter_map(|directive| {
            let directive = directive.trim();
            if directive.is_empty() {
                return None;
            }
            let mut pieces = directive.split_whitespace();
            let name = pieces.next()?.to_string();
            let value = pieces.collect::<Vec<_>>().join(" ");
            Some((name, value))
        })
        .collect()
}

/// 在 `指令 -> 取值` 列表里查找某条指令的取值
fn directive_value<'a>(parsed: &'a [(String, String)], name: &str) -> Option<&'a str> {
    parsed
        .iter()
        .find(|(directive, _)| directive == name)
        .map(|(_, value)| value.as_str())
}

/// 读取 `tauri.conf.json` 里的 `app.security.csp`
fn tauri_conf_csp() -> String {
    let raw = include_str!("../../tauri.conf.json");
    let json: serde_json::Value = serde_json::from_str(raw).expect("tauri.conf.json 应是合法 JSON");
    json["app"]["security"]["csp"]
        .as_str()
        .expect("tauri.conf.json 应配置 app.security.csp")
        .to_string()
}

/// 两侧**都显式写出**的指令必须逐字一致（[`CSP_SHARED_DIRECTIVES`]）
///
/// 这是整套配置里**唯一**能让应用整体不可用的一处：两侧各写一份 CSP 是结构性事实
/// （一份给 `tauri-codegen`、一份给内嵌 HTTP 服务器），但"哪条指令该取什么值"是同一个
/// 决策。漏掉或改错时，前端资源会在桌面端正常、在浏览器端被拦（或反之），而
/// `pnpm check` / `cargo build` 都不会报错，只有真机打开网页端才会暴露。
#[test]
fn csp_directives_written_on_both_sides_match() {
    let tauri_csp = parse_csp(&tauri_conf_csp());
    let web_csp = parse_csp(&super::response::frontend_csp("test-nonce"));

    assert!(
        !CSP_SHARED_DIRECTIVES.is_empty(),
        "共享指令表不应为空，否则本测试形同虚设"
    );

    for (directive, expected) in CSP_SHARED_DIRECTIVES {
        for (label, parsed) in [("tauri.conf.json", &tauri_csp), ("浏览器侧", &web_csp)] {
            let actual = directive_value(parsed, directive)
                .unwrap_or_else(|| panic!("{label} 的 CSP 缺少 {directive} 指令"));
            assert_eq!(
                actual, *expected,
                "{label} 的 {directive} 取值与 CSP_SHARED_DIRECTIVES 不一致：\
                 期望 {expected:?}，实际 {actual:?}"
            );
        }
    }
}

/// 浏览器侧显式收紧的指令不得比"桌面端的 `default-src` 兜底"更松
///
/// 桌面端的 `tauri.conf.json` 只显式写 `default-src` / `style-src` / `img-src` /
/// `media-src`，其余指令靠 `default-src 'self'` 兜底；浏览器侧没有 codegen，必须
/// 把它们全部显式写出。这条断言防的是"有人改 CSP 时顺手放宽了浏览器侧"——
/// 例如给 `object-src` 加上 `*`、给 `base-uri` 放行任意来源。
///
/// 允许的取值只有 `'self'` 与 `'none'`（以及 `script-src` 额外带一个 nonce）。
#[test]
fn web_csp_is_no_looser_than_default_src() {
    let web_csp = parse_csp(&super::response::frontend_csp("test-nonce"));

    assert!(
        !WEB_CSP_HARDENED_DIRECTIVES.is_empty(),
        "收紧指令表不应为空，否则本测试形同虚设"
    );

    for directive in WEB_CSP_HARDENED_DIRECTIVES {
        let value = directive_value(&web_csp, directive)
            .unwrap_or_else(|| panic!("浏览器侧 CSP 应显式声明 {directive}"));

        // script-src 允许 'self' + nonce：先剥掉 nonce 源
        let without_nonce: Vec<&str> = value
            .split_whitespace()
            .filter(|source| !source.starts_with("'nonce-"))
            .collect();
        assert_eq!(
            without_nonce.len(),
            1,
            "浏览器侧 {directive} 除 nonce 外应只有一个来源：{value:?}"
        );
        assert!(
            without_nonce[0] == "'self'" || without_nonce[0] == "'none'",
            "浏览器侧 {directive} 的取值只能是 'self' 或 'none'（script-src 可再加 nonce），\
             否则比桌面端的 default-src 'self' 更松：{value:?}"
        );
    }
}

/// 浏览器侧 CSP 的 `script-src` 必须恰好是 `'self'` + 一个 nonce
///
/// 这条与桌面端无关：桌面端依赖 codegen 注入哈希，浏览器端依赖每次请求的随机 nonce。
/// 防的是"有人为了修某个 inline script 报错，直接把 `'unsafe-inline'` 加进 script-src"
/// ——那等同于关掉 XSS 防护。
#[test]
fn web_csp_script_src_uses_self_and_nonce_only() {
    let parsed = parse_csp(&super::response::frontend_csp("test-nonce"));
    let script_src =
        directive_value(&parsed, "script-src").expect("浏览器侧 CSP 应包含 script-src");

    assert!(
        !script_src.contains("'unsafe-inline'"),
        "浏览器侧 script-src 不应放行 'unsafe-inline'（实际 {script_src:?}）"
    );
    assert_eq!(
        script_src, "'self' 'nonce-test-nonce'",
        "浏览器侧 script-src 应恰好为 'self' + 注入的 nonce"
    );
}

/// 桌面端 CSP 也不许出现 `unsafe-inline` 形式的脚本放行
///
/// `tauri.conf.json` 不写 `script-src`（见 `CSP_SHARED_DIRECTIVES` 的说明），
/// 因此脚本策略回落到 `default-src 'self'`。这里断言 `default-src` 不是
/// `'unsafe-inline'`、也没有被塞进 `script-src`——一旦有人为图省事加上
/// `script-src 'unsafe-inline'`，等于把构建期注入的 sha256 哈希机制整个作废。
#[test]
fn tauri_conf_never_allows_unsafe_inline_scripts() {
    let parsed = parse_csp(&tauri_conf_csp());

    let default_src = directive_value(&parsed, "default-src").expect("应有 default-src");
    assert!(
        !default_src.contains("'unsafe-inline'"),
        "tauri.conf.json 的 default-src 不应放行 'unsafe-inline'（实际 {default_src:?}）"
    );

    if let Some(script_src) = directive_value(&parsed, "script-src") {
        assert!(
            !script_src.contains("'unsafe-inline'"),
            "tauri.conf.json 的 script-src 一旦出现，就不应放行 'unsafe-inline'\
             （实际 {script_src:?}）"
        );
    }
}

/// 浏览器侧 CSP 不应包含桌面端专用的 `asset:` 协议
///
/// 反向断言：`asset:` 只在 Tauri webview 里存在，浏览器无法理解这个 scheme；
/// 一旦有人"统一"两份 CSP 时把 `asset:` 抄过来，浏览器会因无法识别的 scheme
/// 而拒绝整条 `media-src`（不是忽略该源），视频直接全部播不出来。
#[test]
fn web_csp_excludes_tauri_asset_protocol() {
    let csp = super::response::frontend_csp("test-nonce");
    assert!(
        !csp.contains("asset:"),
        "浏览器侧 CSP 不应包含桌面端专用的 asset: 协议：{csp}"
    );
}

/// 前端 `DEFAULT_SHARE_PORT` 与后端 `constants::DEFAULT_SHARE_PORT` 必须一致
///
/// 不一致时表现为"界面显示一个端口、服务器实际监听另一个"，用户按界面地址访问必然失败。
#[test]
fn share_port_matches_frontend_config() {
    let config_ts = include_str!("../../../src/lib/config.ts");
    let expected = format!("export const DEFAULT_SHARE_PORT = {};", DEFAULT_SHARE_PORT);
    assert!(
        config_ts.contains(&expected),
        "src/lib/config.ts 应包含 `{expected}`，实际内容：\n{config_ts}"
    );
}

/// 从 `export const NAME = <数字>;` 中取出数字值（忽略下划线分组符）
///
/// TS 允许 `1_048_576` 这种可读写法，而 Rust `{}` 输出 `1048576`。
/// 直接字符串比对会因分组符误报，因此这里把两侧都归一成纯数字再比。
fn const_number(source: &str, name: &str) -> Option<u64> {
    let marker = format!("export const {} =", name);
    let start = source.find(&marker)? + marker.len();
    let rest = &source[start..];
    let end = rest.find(';')?;
    let raw: String = rest[..end]
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    raw.parse().ok()
}

/// 前端 `MIN_VIDEO_FILE_SIZE_BYTES` 与后端 `constants::MIN_VIDEO_FILE_SIZE_BYTES` 必须一致
///
/// 前端只用它来生成"已跳过 N 个过小文件（小于 …）"的文案。两处不一致时用户看到的
/// 阈值提示与实际过滤阈值对不上，比不提示更让人困惑。
#[test]
fn min_video_size_matches_frontend_config() {
    let config_ts = include_str!("../../../src/lib/config.ts");
    let actual = const_number(config_ts, "MIN_VIDEO_FILE_SIZE_BYTES")
        .expect("src/lib/config.ts 应定义 MIN_VIDEO_FILE_SIZE_BYTES 数字常量");
    assert_eq!(
        actual, MIN_VIDEO_FILE_SIZE_BYTES,
        "src/lib/config.ts 的 MIN_VIDEO_FILE_SIZE_BYTES 与 constants.rs 不一致"
    );
}

/// Vite 的 dev / HMR 端口必须与导航守卫放行的端口一致
///
/// `lib.rs` 的导航守卫按端口白名单放行 dev server；`vite.config.js` 改了端口而守卫
/// 没跟着改，开发时整个页面会被守卫拦成一片空白，且不会报任何错。
///
/// 守卫侧只检查"是否引用了这两个常量"而不是字面端口：字面值写在 `constants.rs`
/// 一处，守卫用 `constants::DEV_SERVER_PORT` / `DEV_HMR_PORT` 拼接，
/// 因此"改了常量却忘了守卫"在类型层面就不可能发生——真正会漏的是忘记同步
/// `vite.config.js`，那由下面第一段断言把住。
#[test]
fn dev_ports_match_vite_config() {
    let vite_config = include_str!("../../../vite.config.js");

    for port in [DEV_SERVER_PORT, DEV_HMR_PORT] {
        assert!(
            vite_config.contains(&format!("port: {port}")),
            "vite.config.js 应配置 port: {port}，实际内容：\n{vite_config}"
        );
    }

    let lib_rs = include_str!("../lib.rs");
    assert!(
        lib_rs.contains("constants::DEV_SERVER_PORT"),
        "lib.rs 的导航守卫应引用 constants::DEV_SERVER_PORT 而不是字面端口"
    );
    assert!(
        lib_rs.contains("constants::DEV_HMR_PORT"),
        "lib.rs 的导航守卫应引用 constants::DEV_HMR_PORT 而不是字面端口"
    );
    assert!(
        !lib_rs.contains("localhost:1420") && !lib_rs.contains("localhost:1421"),
        "lib.rs 不应再硬编码 dev 端口字面值"
    );
}
