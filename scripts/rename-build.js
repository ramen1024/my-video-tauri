import { copyFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(__dirname, '..');

const pkg = require(resolve(rootDir, 'package.json'));
const version = pkg.version;

const exePath = resolve(rootDir, 'src-tauri/target/release/video-scanner.exe');
const newName = `视频扫描器_${version}_x64.exe`;
const newPath = resolve(rootDir, 'src-tauri/target/release', newName);

if (existsSync(exePath)) {
    copyFileSync(exePath, newPath);
    console.log(`✅ 已生成: ${newName}`);
} else {
    console.error('❌ 找不到编译产物: video-scanner.exe');
    process.exit(1);
}
