#!/usr/bin/env node
/**
 * 前后端配置一致性检查
 *
 * 项目里有若干"同一个决策同时写在 Rust 与 TypeScript 两侧"的常量。Rust 侧的
 * `src-tauri/src/server/csp_tests.rs` 已经覆盖了 Rust → TS 方向的比对；本脚本补上
 * 另外两个 Rust 侧不方便表达、但同样会静默出错的约束：
 *
 * 1. `INLINE_PLAYABLE_EXTENSIONS`（前端"优先用内置播放器尝试"清单）必须是
 *    `VIDEO_TYPES`（后端"可扫描"清单）的**子集**——前端列出一个后端根本不扫描的
 *    扩展名毫无意义（永远不会有这种文件出现在列表里），列漏了则会把本可内联播放的
 *    文件无谓地丢给系统播放器。
 * 2. 两份清单都不允许出现重复项或大写项（比较前统一小写，写错大小写会静默失效）。
 *
 * 与前一条方向的重叠是有意的：Rust 测试读不了 TS 的数组字面量，TS 脚本读不了
 * Rust 的 `&[(&str, &str)]`（不引入新依赖的前提下），两边各校验自己能读的那半，
 * 合起来覆盖完整。
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** 读取文件，失败时直接抛出（配置缺失不应被当成"检查通过"） */
function read(relativePath) {
  const absolute = resolve(root, relativePath);
  try {
    return readFileSync(absolute, "utf8");
  } catch (error) {
    throw new Error(`无法读取 ${relativePath}：${error.message}`);
  }
}

/** 从 `const NAME = [...];` 形式的字面量里取出字符串数组 */
function extractStringArray(source, name) {
  const match = source.match(new RegExp(`const\\s+${name}\\s*=\\s*\\[([^\\]]*)\\]`));
  if (!match) {
    throw new Error(`未找到 ${name} 的数组字面量`);
  }
  return [...match[1].matchAll(/"([^"]*)"/g)].map((m) => m[1]);
}

const failures = [];

/** 断言，失败时记录而不是立即抛出，让一次运行报出全部问题 */
function check(condition, message) {
  if (!condition) failures.push(message);
}

// ---------------- 读取两侧清单 ----------------

const formatSource = read("src/lib/utils/format.ts");
const constantsSource = read("src-tauri/src/constants.rs");

const inlinePlayable = extractStringArray(formatSource, "INLINE_PLAYABLE_EXTENSIONS");

// Rust 侧是 `&[(&str, &str)]`，取每对的第一个字符串
const videoTypesBlock = constantsSource.match(/pub const VIDEO_TYPES[^=]*=\s*&\[([\s\S]*?)\];/);
if (!videoTypesBlock) {
  throw new Error("未在 constants.rs 中找到 VIDEO_TYPES 定义");
}
const videoTypes = [...videoTypesBlock[1].matchAll(/\(\s*"([^"]*)"/g)].map((m) => m[1]);

// ---------------- 断言 ----------------

check(inlinePlayable.length > 0, "INLINE_PLAYABLE_EXTENSIONS 不应为空");
check(videoTypes.length > 0, "VIDEO_TYPES 不应为空");

// 1. 前端清单必须是后端清单的子集
const videoTypeSet = new Set(videoTypes);
for (const ext of inlinePlayable) {
  check(
    videoTypeSet.has(ext),
    `INLINE_PLAYABLE_EXTENSIONS 含有后端不扫描的扩展名 "${ext}"：` +
      `该格式永远不会有文件进入列表，列在这里没有意义（后端清单：${videoTypes.join(", ")}）`
  );
}

// 2. 两份清单都不允许重复
for (const [label, list] of [
  ["INLINE_PLAYABLE_EXTENSIONS", inlinePlayable],
  ["VIDEO_TYPES", videoTypes],
]) {
  const unique = new Set(list);
  check(
    unique.size === list.length,
    `${label} 存在重复项：${list.join(", ")}`
  );
}

// 3. 两份清单都必须是小写：比较前统一 to_lowercase()，写大写会静默失效
for (const [label, list] of [
  ["INLINE_PLAYABLE_EXTENSIONS", inlinePlayable],
  ["VIDEO_TYPES", videoTypes],
]) {
  const offenders = list.filter((ext) => ext !== ext.toLowerCase());
  check(
    offenders.length === 0,
    `${label} 含有大写或非小写项（比较前会 to_lowercase，写大写必然匹配不上）：` +
      offenders.join(", ")
  );
}

// 4. 前端清单必须显式声明"这不是保证"——它是启发式，回退逻辑依赖这一点
check(
  /启发式|not a guarantee|值得一试/.test(formatSource),
  "src/lib/utils/format.ts 应说明 INLINE_PLAYABLE_EXTENSIONS 是启发式清单而非播放保证"
);

// ---------------- 输出 ----------------

if (failures.length > 0) {
  console.error("前后端配置一致性检查失败：");
  for (const failure of failures) {
    console.error(`  - ${failure}`);
  }
  process.exit(1);
}

console.log(
  `前后端配置一致性检查通过（内联播放清单 ${inlinePlayable.length} 项 ⊂ 后端可扫描清单 ${videoTypes.length} 项）`
);
