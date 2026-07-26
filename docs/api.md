# HTTP API 文档

本文档描述“视频扫描器”局域网共享功能对外暴露的 HTTP 端点。所有请求均通过应用内置的 tiny_http 服务器处理，默认监听由应用界面显示的本地 IP 与端口。

## 认证说明

当用户在设置中开启密码保护后，除以下端点外，其余所有请求都必须携带有效的 `session_token` Cookie：

- `POST /auth`
- `GET /login`
- `GET /login.html`

未携带有效 Cookie 的请求会被重定向到 `/login` 登录页面。

Session Cookie 属性：`HttpOnly; SameSite=Strict; Path=/`，有效期与服务器端 session 一致。

---

## 端点列表

### `POST /auth`

密码认证端点。认证成功后会设置 session cookie。

**请求头：**

```text
Content-Type: application/json
```

**请求体：**

```json
{
  "password": "1234"
}
```

**响应示例（成功，HTTP 200）：**

```json
{
  "success": true,
  "message": "认证成功",
  "token": "<session_token>"
}
```

响应头包含 `Set-Cookie: session_token=<token>; Path=/; Max-Age=...; HttpOnly; SameSite=Strict`。

**响应示例（失败，HTTP 401）：**

```json
{
  "success": false,
  "message": "密码错误"
}
```

---

### `GET /videos`

返回当前共享文件夹中的视频列表 JSON。

**响应示例（HTTP 200）：**

```json
[
  {
    "relative_path": "movies/demo.mp4",
    "size": 123456789,
    "name": "demo",
    "extension": "mp4"
  }
]
```

响应头包含 `ETag`，客户端可携带 `If-None-Match` 请求头。当数据未发生变化时，服务器返回 `304 Not Modified`，响应体为空。

---

### `GET /refresh`

触发后台重新扫描共享文件夹，并异步更新视频列表。

**响应示例（成功开始扫描，HTTP 202）：**

```json
{
  "success": true,
  "message": "刷新已开始"
}
```

其他可能情况：

- 已有刷新任务正在进行 → HTTP 429
- 刷新过于频繁（冷却中） → HTTP 429
- 未设置共享文件夹 → HTTP 400

---

### `GET /refresh-status`

查询最近一次刷新任务的结果。

**响应示例（已产生结果，HTTP 200）：**

```json
{
  "success": true,
  "message": "视频列表已刷新"
}
```

**响应示例（无刷新记录，HTTP 200）：**

```json
{
  "success": true,
  "message": "无刷新记录"
}
```

---

### `GET /video/<relative_path>`

视频文件流式服务，支持 HTTP `Range` 请求，可用于浏览器拖动进度条播放。

- `<relative_path>` 为视频文件相对于共享文件夹的路径，需要进行 URL 编码。
- 请求头可包含 `Range: bytes=<start>-<end>`。
- 返回 `200 OK`（完整内容）或 `206 Partial Content`（Range 请求）。
- 响应头包含 `Accept-Ranges: bytes` 与 `Content-Type`（根据扩展名自动推断）。

**示例请求：**

```text
GET /video/movies/demo.mp4 HTTP/1.1
Range: bytes=0-1048575
```

---

## 其他说明

- `/` 与 `/index.html` 返回视频列表的 HTML 页面。
- `/login` 与 `/login.html` 返回登录页面。
- 其他未匹配路径返回 `404 Not found`。
