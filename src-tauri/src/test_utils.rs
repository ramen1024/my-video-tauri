//! 测试专用助手（仅在 cargo test 下编译，不进入发布产物）
//!
//! 集中各模块单元测试重复使用的临时目录、样例数据与 HTTP 客户端逻辑，
//! 替代此前复制到多个 `mod tests` 中的同名函数。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 在系统临时目录下创建带唯一后缀的目录，测试结束时由调用方自行清理
pub fn make_temp_dir(prefix: &str) -> PathBuf {
    let name = format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(name);
    fs::create_dir_all(&path).expect("创建临时目录失败");
    path
}

/// 写入指定大小的全零文件，用于构造尺寸达标（≥ MIN_VIDEO_FILE_SIZE_BYTES）的假视频
pub fn create_test_video(path: &Path, size: u64) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("创建父目录失败");
    }
    let mut file = fs::File::create(path).expect("创建测试视频文件失败");
    let buf = vec![0u8; 8192];
    let mut remaining = size;
    while remaining > 0 {
        let chunk = std::cmp::min(remaining, buf.len() as u64) as usize;
        file.write_all(&buf[..chunk]).expect("写入测试视频数据失败");
        remaining -= chunk as u64;
    }
}

/// 构造指定相对路径、大小、修改时间的样例视频条目
///
/// 文件名取自相对路径的最后一段，扩展名固定为 mp4。
pub fn sample_video(relative_path: &str, size: u64, modified: Option<&str>) -> crate::VideoFile {
    crate::VideoFile {
        name: relative_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(relative_path)
            .to_string(),
        path: format!("C:\\videos\\{}", relative_path),
        relative_path: relative_path.to_string(),
        size,
        modified: modified.map(|m| m.to_string()),
        extension: "mp4".to_string(),
    }
}

/// 测试用前端资源目录：`index.html` + 哈希命名的 `_app/immutable` 资源 + favicon + 一个非资源文件
///
/// 用磁盘目录而非真实构建产物，使 HTTP 路由测试不依赖"先构建前端"这一前置步骤。
pub fn make_frontend_dist(prefix: &str) -> PathBuf {
    let root = make_temp_dir(prefix);
    fs::create_dir_all(root.join("_app/immutable/entry")).expect("创建前端资源目录失败");
    fs::write(
        root.join("index.html"),
        b"<!doctype html><html><head><title>t</title></head><body><script>__x=1</script></body></html>",
    )
    .expect("写入 index.html 失败");
    fs::write(
        root.join("_app/immutable/entry/app.abc123.js"),
        b"export const x = 1;",
    )
    .expect("写入 js 资源失败");
    fs::write(root.join("_app/env.js"), b"export const env = {};").expect("写入 env.js 失败");
    fs::write(root.join("favicon.png"), b"\x89PNG\r\n\x1a\n").expect("写入 favicon 失败");
    root
}

/// 一次 HTTP 响应的解析结果（仅支持文本响应体，足够用于路由与头部断言）
#[derive(Debug)]
pub struct HttpResponse {
    /// 状态码
    pub status: u16,
    /// 响应头（字段名保持原样，比较时请自行大小写归一）
    pub headers: Vec<(String, String)>,
    /// 响应体
    pub body: String,
}

impl HttpResponse {
    /// 按名称读取响应头（大小写不敏感）
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// 向本地测试服务器发送一个 HTTP 请求并解析响应
///
/// 用裸 `TcpStream` 而不是引入 HTTP 客户端依赖：测试只关心状态行、头部与文本正文。
/// 未显式提供 `Host` 时默认使用 `127.0.0.1`（服务端只放行本机 IP / localhost）。
pub fn http_request(
    port: u16,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> HttpResponse {
    http_request_with_body(port, method, path, extra_headers, None)
}

/// 同 [`http_request`]，但可携带请求体（用于 `POST /auth`）
pub fn http_request_with_body(
    port: u16,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: Option<&str>,
) -> HttpResponse {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("连接测试服务器失败");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("设置读超时失败");

    let has_host = extra_headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("Host"));

    let mut request = format!("{} {} HTTP/1.1\r\n", method, path);
    if !has_host {
        request.push_str("Host: 127.0.0.1\r\n");
    }
    for (name, value) in extra_headers {
        request.push_str(&format!("{}: {}\r\n", name, value));
    }
    if let Some(body) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    request.push_str("Connection: close\r\n\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }

    stream.write_all(request.as_bytes()).expect("写入请求失败");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("读取响应失败");
    parse_http_response(&raw)
}

/// 在超时内并行等待所有线程退出，返回成功退出的线程数
///
/// 与 `stop_share_server` 的做法一致：不用无超时的 `join`，否则实现出错时
/// 测试会永久挂起而不是失败。
pub fn join_all_with_timeout(
    handles: Vec<std::thread::JoinHandle<()>>,
    timeout: Duration,
) -> usize {
    let count = handles.len();
    let (tx, rx) = std::sync::mpsc::channel();
    for handle in handles {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = tx.send(());
        });
    }
    drop(tx);

    let deadline = std::time::Instant::now() + timeout;
    let mut finished = 0;
    for _ in 0..count {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if rx.recv_timeout(remaining).is_err() {
            break;
        }
        finished += 1;
    }
    finished
}

/// 解析 HTTP/1.1 响应（状态行 + 头部 + 正文）
fn parse_http_response(raw: &[u8]) -> HttpResponse {
    let text = String::from_utf8_lossy(raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));

    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();

    HttpResponse {
        status,
        headers,
        body: body.to_string(),
    }
}
