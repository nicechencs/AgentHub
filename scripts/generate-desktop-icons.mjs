#!/usr/bin/env node
/**
 * Generate desktop-only app icons for Windows / macOS / Linux.
 *
 * Tauri's `icon` CLI always emits mobile + Windows Store assets; this wrapper
 * runs it then strips those platforms so the tree stays desktop-only.
 *
 * Usage:
 *   pnpm icons
 *   node scripts/generate-desktop-icons.mjs [input.svg|png]
 *
 * Source default: src-tauri/app-icon.svg
 * Output:         src-tauri/icons/  (desktop set only)
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');
const iconsDir = path.join(root, 'src-tauri', 'icons');
const defaultInput = path.join(root, 'src-tauri', 'app-icon.svg');

/** Files/dirs that tauri icon emits for mobile / store — not used by this app. */
const STRIP_NAMES = new Set([
  'android',
  'ios',
  'StoreLogo.png',
  'Square30x30Logo.png',
  'Square44x44Logo.png',
  'Square71x71Logo.png',
  'Square89x89Logo.png',
  'Square107x107Logo.png',
  'Square142x142Logo.png',
  'Square150x150Logo.png',
  'Square284x284Logo.png',
  'Square310x310Logo.png',
]);

/** Must remain after generation (matches tauri.conf.json + common desktop extras). */
const REQUIRED = [
  '32x32.png',
  '128x128.png',
  '128x128@2x.png',
  'icon.icns',
  'icon.ico',
];

function fail(msg) {
  console.error(`[icons] ${msg}`);
  process.exit(1);
}

function stripNonDesktop() {
  if (!fs.existsSync(iconsDir)) {
    fail(`icons dir missing: ${iconsDir}`);
  }

  const removed = [];
  for (const name of fs.readdirSync(iconsDir)) {
    if (!STRIP_NAMES.has(name) && !name.startsWith('Square')) continue;
    const full = path.join(iconsDir, name);
    fs.rmSync(full, { recursive: true, force: true });
    removed.push(name);
  }

  // Catch any future Square* / store-style names not in the fixed set.
  for (const name of fs.readdirSync(iconsDir)) {
    if (/^Square.+\.png$/i.test(name) || /^StoreLogo/i.test(name)) {
      fs.rmSync(path.join(iconsDir, name), { force: true });
      if (!removed.includes(name)) removed.push(name);
    }
  }

  return removed;
}

function assertDesktopSet() {
  const missing = REQUIRED.filter((f) => !fs.existsSync(path.join(iconsDir, f)));
  if (missing.length) {
    fail(`missing required desktop icons: ${missing.join(', ')}`);
  }
}

function main() {
  const inputArg = process.argv[2];
  const input = path.resolve(root, inputArg ?? defaultInput);
  if (!fs.existsSync(input)) {
    fail(`source icon not found: ${input}`);
  }

  console.log(`[icons] source: ${path.relative(root, input)}`);
  console.log(`[icons] running: pnpm tauri icon ${path.relative(root, input)}`);

  const isWin = process.platform === 'win32';
  const result = spawnSync(
    isWin ? 'pnpm.cmd' : 'pnpm',
    ['tauri', 'icon', input],
    { cwd: root, stdio: 'inherit', shell: isWin },
  );

  if (result.error) fail(result.error.message);
  if (result.status !== 0) fail(`tauri icon exited with code ${result.status ?? 'unknown'}`);

  const removed = stripNonDesktop();
  assertDesktopSet();

  const kept = fs
    .readdirSync(iconsDir)
    .filter((n) => {
      const p = path.join(iconsDir, n);
      return fs.statSync(p).isFile();
    })
    .sort();

  console.log(`[icons] stripped non-desktop: ${removed.length ? removed.join(', ') : '(none)'}`);
  console.log(`[icons] desktop set (${kept.length}): ${kept.join(', ')}`);
  console.log('[icons] done — use `pnpm icons` (not bare `pnpm tauri icon`) to stay desktop-only');
}

main();
