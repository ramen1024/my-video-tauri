#!/usr/bin/env node
/**
 * 前端构建产物形状校验
 *
 * 网页端由 Rust 内嵌 HTTP 服务器直接提供 `build/` 里的这份产物
 * （见 `src-tauri/src/server/handler.rs`）。本脚本在 `pnpm build` 之后运行，
 * 把"服务端对产物形状的假设"变成构建期就会失败的断言，避免上游
 * SvelteKit / Vite 升级后悄悄打破它：
 *
 * 1. `index.html` 内的每个内联 `<script>` 必须是裸 `<script>` 标签——
 *    服务端用精确字符串替换注入 CSP nonce；一旦上游改成
 *    `<script type="module">` 之类的写法，注入就会失效，页面脚本会被 CSP 拦下
 *    （表现为白屏），而这是运行时才暴露的问题。
 * 2. `index.html` 引用的每个资源路径都必须真的存在于 `build/` 中，
 *    否则网页端会出现 404 资源。
 * 3. base 必须为空字符串：产物会被挂在服务器根路径下。
 *
 * 用法：node scripts/check-web-build.mjs
 */

import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const distDir = join(repoRoot, "build");
const indexPath = join(distDir, "index.html");

/** @type {string[]} */
const failures = [];

/** 记录一条断言 */
function check(condition, message) {
  if (!condition) failures.push(message);
}

if (!existsSync(indexPath)) {
  console.error(`[check-web-build] 未找到构建产物: ${indexPath}`);
  console.error("[check-web-build] 请先执行 pnpm build");
  process.exit(1);
}

const html = readFileSync(indexPath, "utf8");

// 1. 内联脚本必须写成裸 <script>，否则服务端的 nonce 注入覆盖不到
const scriptTags = html.match(/<script\b[^>]*>/gi) ?? [];
const inlineTags = scriptTags.filter((tag) => !/\ssrc=/i.test(tag));
const nonConforming = inlineTags.filter((tag) => tag !== "<script>");
check(
  nonConforming.length === 0,
  `内联 <script> 必须写成裸标签 <script>，否则 server/handler.rs 的 nonce 注入覆盖不到，` +
    `脚本会被 CSP 拦下并白屏。实际写法: ${nonConforming.join(", ") || "(无)"}`,
);
check(
  inlineTags.length > 0,
  "index.html 中没有任何内联 <script>：SvelteKit 的启动脚本形态可能已变化，请核对 nonce 注入逻辑",
);

// 2. base 必须为空：产物挂在服务器根路径
check(
  html.includes('base: ""'),
  `index.html 的 SvelteKit base 不是空字符串，产物无法挂在服务器根路径下`,
);

// 3. 引用的资源必须存在
const referenced = [...html.matchAll(/(?:href|src)="(\/[^"]*)"/gi)].map((m) => m[1]);
check(referenced.length > 0, "index.html 未引用任何以 / 开头的资源，产物结构可能已变化");

for (const ref of referenced) {
  const relative = ref.split(/[?#]/)[0].replace(/^\/+/, "");
  if (!relative) continue;
  const target = join(distDir, relative);
  check(existsSync(target), `index.html 引用的资源在产物中不存在: ${ref}`);
}

// 4. 内容哈希目录必须存在，否则服务端的强缓存策略会退化为短缓存
const immutableDir = join(distDir, "_app", "immutable");
check(
  existsSync(immutableDir) && statSync(immutableDir).isDirectory(),
  "缺少 _app/immutable 目录：静态资源的强缓存策略将失去依据",
);

if (failures.length > 0) {
  console.error("[check-web-build] 构建产物不满足服务端假设：");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}

console.log(
  `[check-web-build] 通过（内联脚本 ${inlineTags.length} 个，引用资源 ${referenced.length} 个）`,
);
