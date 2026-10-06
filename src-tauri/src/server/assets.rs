//! 前端静态资源访问
//!
//! 桌面端与网页端**共用同一份 SvelteKit 构建产物**（`tauri.conf.json` 的
//! `frontendDist`，即仓库根目录的 `build/`）：桌面端由 Tauri 自己加载，
//! 网页端由内嵌 HTTP 服务器通过本模块提供。
//!
//! 生产环境从可执行文件内嵌的资源读取（[`FrontendAssets::embedded`]），
//! 测试环境直接从磁盘目录读取（[`FrontendAssets::directory`]），
//! 因此 HTTP 路由逻辑可以在不构建前端的情况下被完整测试。

#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;

use crate::utils::urlencoding_decode;

/// 一份前端静态资源
pub struct FrontendAsset {
    /// 资源内容
    pub bytes: Vec<u8>,
    /// 响应用的 MIME 类型
    pub mime_type: String,
}

/// 资源加载器：给定 URL 路径返回资源，未命中返回 `None`
type AssetLoader = dyn Fn(&str) -> Option<FrontendAsset> + Send + Sync;

/// 前端静态资源的来源
///
/// 加载器以**类型擦除的闭包**保存，而不是直接存 `tauri::AppHandle`：`AppHandle<Wry>` /
/// `AssetResolver<Wry>` 一旦出现在单元测试可达的类型里，链接器就会把整条 tao/wry
/// 桌面栈拉进 `cargo test` 的可执行文件。实测这会带来两个问题：
///
/// 1. 测试二进制凭空多出约 2MB 代码（user32 / ole32 / dwmapi / gdi32 / comctl32 全套 GUI 导入）；
/// 2. 它会导入 `comctl32!TaskDialogIndirect`——该符号只由带 Common Controls v6 清单的
///    comctl32 提供，而测试可执行文件没有清单，加载时直接以
///    `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) 失败，整个测试进程起不来。
///
/// 擦除类型后，只有 `run()` 的 setup 会实例化那部分代码，测试构建保持精简。
#[derive(Clone)]
pub struct FrontendAssets {
    loader: Arc<AssetLoader>,
}

impl FrontendAssets {
    /// 生产：从 Tauri 在构建期打包进可执行文件的 `frontendDist` 读取
    pub fn embedded(app: tauri::AppHandle) -> Self {
        Self {
            loader: Arc::new(move |url_path| {
                let relative = normalize_asset_path(url_path)?;
                app.asset_resolver()
                    .get(relative)
                    .map(|asset| FrontendAsset {
                        bytes: asset.bytes,
                        mime_type: asset.mime_type,
                    })
            }),
        }
    }

    /// 测试：直接读取磁盘目录
    ///
    /// 仅用于单元测试，使 HTTP 路由的测试不依赖 "先构建前端" 这一前置步骤。
    #[cfg(test)]
    pub fn directory(root: PathBuf) -> Self {
        Self {
            loader: Arc::new(move |url_path| {
                let relative = normalize_asset_path(url_path)?;
                let bytes = std::fs::read(root.join(&relative)).ok()?;
                Some(FrontendAsset {
                    mime_type: mime_type_for(&relative).to_string(),
                    bytes,
                })
            }),
        }
    }

    /// 按 URL 路径查找资源，未命中返回 `None`
    pub fn get(&self, url_path: &str) -> Option<FrontendAsset> {
        (self.loader)(url_path)
    }
}

/// 将请求路径规范化为 `frontendDist` 内的相对路径
///
/// 返回 `None` 表示路径非法，调用方应拒绝该请求：
/// - `..` 会逃出资源根目录
/// - 反斜杠与冒号在 Windows 上是路径分隔符 / 盘符前缀，必须拒绝
///
/// 根路径（空路径或 `/`）映射为 `index.html`。
pub(crate) fn normalize_asset_path(url_path: &str) -> Option<String> {
    // 去掉查询串与片段后再百分号解码：浏览器会对非 ASCII 文件名做编码
    let path_only = url_path.split(['?', '#']).next().unwrap_or("");
    let decoded = urlencoding_decode(path_only);

    let mut segments: Vec<&str> = Vec::new();
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') || segment.contains(':') {
            return None;
        }
        segments.push(segment);
    }

    if segments.is_empty() {
        return Some("index.html".to_string());
    }
    Some(segments.join("/"))
}

/// 按扩展名推断 MIME 类型
///
/// 仅 [`FrontendAssets::directory`] 需要（内嵌资源的 MIME 由 Tauri 提供）。
#[cfg(test)]
fn mime_type_for(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_root_to_index() {
        assert_eq!(normalize_asset_path("/").as_deref(), Some("index.html"));
        assert_eq!(normalize_asset_path("").as_deref(), Some("index.html"));
        assert_eq!(
            normalize_asset_path("/index.html").as_deref(),
            Some("index.html")
        );
    }

    #[test]
    fn normalize_keeps_nested_paths_and_drops_query() {
        assert_eq!(
            normalize_asset_path("/_app/immutable/entry/app.js?v=1").as_deref(),
            Some("_app/immutable/entry/app.js")
        );
        assert_eq!(
            normalize_asset_path("/favicon.png").as_deref(),
            Some("favicon.png")
        );
        // 百分号编码的路径需要解码后再落盘查找
        assert_eq!(
            normalize_asset_path("/%E8%A7%86%E9%A2%91.js").as_deref(),
            Some("视频.js")
        );
        // 连续斜杠与 `.` 段被折叠
        assert_eq!(
            normalize_asset_path("//_app//./a.js").as_deref(),
            Some("_app/a.js")
        );
    }

    #[test]
    fn normalize_rejects_traversal_and_windows_separators() {
        assert_eq!(normalize_asset_path("/../secrets.txt"), None);
        assert_eq!(normalize_asset_path("/_app/../../etc/passwd"), None);
        assert_eq!(normalize_asset_path("/..%2f..%2fetc"), None);
        assert_eq!(normalize_asset_path("/_app\\..\\x"), None);
        assert_eq!(normalize_asset_path("/C:/Windows/win.ini"), None);
    }

    #[test]
    fn directory_source_reads_files_and_reports_mime() {
        let root = crate::test_utils::make_temp_dir("frontend_assets");
        std::fs::create_dir_all(root.join("_app")).expect("创建子目录失败");
        std::fs::write(root.join("index.html"), b"<html>ok</html>").expect("写入失败");
        std::fs::write(root.join("_app/app.js"), b"console.log(1)").expect("写入失败");

        let assets = FrontendAssets::directory(root.clone());

        let index = assets.get("/").expect("根路径应命中 index.html");
        assert_eq!(index.bytes, b"<html>ok</html>");
        assert_eq!(index.mime_type, "text/html; charset=utf-8");

        let js = assets.get("/_app/app.js").expect("应命中 js 资源");
        assert_eq!(js.mime_type, "text/javascript; charset=utf-8");

        assert!(
            assets.get("/missing.js").is_none(),
            "不存在的资源应返回 None"
        );
        assert!(assets.get("/../../x").is_none(), "非法路径应返回 None");

        let _ = std::fs::remove_dir_all(&root);
    }
}
