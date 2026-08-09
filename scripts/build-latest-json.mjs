#!/usr/bin/env node
/**
 * Build Tauri static updater feed (`latest.json`) from release artifacts.
 *
 * Usage (after `pnpm tauri:build` with signing env set):
 *   node scripts/build-latest-json.mjs \
 *     --version 0.2.0 \
 *     --notes "bugfixes" \
 *     --base-url https://github.com/nicechencs/AgentHub/releases/download/v0.2.0 \
 *     --out latest.json
 *
 * Looks under target/release/bundle for:
 *   nsis/*-setup.exe + .sig
 *   msi/*.msi + .sig
 *   appimage/*.AppImage + .sig
 *   macos/*.app.tar.gz + .sig
 *
 * Platform keys follow Tauri static JSON: windows-x86_64 / linux-x86_64 / darwin-*.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

function parseArgs(argv) {
  const out = {
    version: null,
    notes: '',
    baseUrl: null,
    out: 'latest.json',
    targetDir: path.join(root, 'target', 'release', 'bundle'),
    // Optional when a macOS artifact name does not carry an architecture.
    macArch: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--version') out.version = argv[++i];
    else if (a === '--notes') out.notes = argv[++i] ?? '';
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--out') out.out = argv[++i];
    else if (a === '--target-dir') out.targetDir = path.resolve(argv[++i]);
    else if (a === '--mac-arch') out.macArch = normalizeMacArch(argv[++i]);
    else if (a === '--help' || a === '-h') {
      console.log(`Usage: node scripts/build-latest-json.mjs --version X.Y.Z --base-url URL [--notes TEXT] [--out latest.json] [--mac-arch aarch64|x86_64]`);
      process.exit(0);
    }
  }
  return out;
}

/** Normalize common Rust/Tauri/Node spellings to updater platform keys. */
function normalizeMacArch(value) {
  if (value == null || value === '') return null;
  const normalized = String(value).trim().toLowerCase().replace(/[-_]/g, '');
  if (['aarch64', 'arm64', 'darwinarm64', 'darwinaarch64'].includes(normalized)) {
    return 'darwin-aarch64';
  }
  if (['x8664', 'amd64', 'x64', 'darwinx64', 'darwinx8664'].includes(normalized)) {
    return 'darwin-x86_64';
  }
  throw new Error(`Invalid --mac-arch '${value}', expected aarch64 or x86_64`);
}

function inferMacArch(fileName) {
  const name = fileName.toLowerCase();
  if (/(?:^|[-_.])(aarch64|arm64)(?:[-_.]|$)/.test(name)) return 'darwin-aarch64';
  if (/(?:^|[-_.])(x86_64|x64|amd64)(?:[-_.]|$)/.test(name)) return 'darwin-x86_64';
  return null;
}

function readSig(filePath) {
  return fs.readFileSync(filePath, 'utf8').trim();
}

function findFirst(dir, predicate) {
  if (!fs.existsSync(dir)) return null;
  const names = fs.readdirSync(dir);
  for (const name of names) {
    const full = path.join(dir, name);
    if (predicate(name, full)) return full;
  }
  return null;
}

function findAll(dir, predicate) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir)
    .filter((name) => predicate(name, path.join(dir, name)))
    .map((name) => path.join(dir, name));
}

function addPlatform(platforms, key, artifactPath, baseUrl) {
  if (!artifactPath) return;
  const sigPath = `${artifactPath}.sig`;
  if (!fs.existsSync(sigPath)) {
    console.warn(`skip ${key}: missing signature ${sigPath}`);
    return;
  }
  const fileName = path.basename(artifactPath);
  platforms[key] = {
    signature: readSig(sigPath),
    url: `${baseUrl.replace(/\/$/, '')}/${fileName}`,
  };
  console.log(`+ ${key}: ${fileName}`);
}

/**
 * Select signed macOS artifacts without assuming that the first directory
 * entry is Intel. Explicit architecture markers always win; generic artifacts
 * require --mac-arch because the Node host may differ from the build target.
 */
function macArtifacts(macosDir, requestedArch = null) {
  const files = findAll(macosDir, (name) => name.endsWith('.app.tar.gz'));
  const selected = [];
  const generic = [];
  for (const file of files) {
    const arch = inferMacArch(path.basename(file));
    if (arch) selected.push({ key: arch, file });
    else generic.push(file);
  }

  const fallback = requestedArch ? normalizeMacArch(requestedArch) : null;
  if (generic.length === 1) {
    if (!fallback) {
      throw new Error(
        `cannot determine architecture for ${path.basename(generic[0])}; pass --mac-arch aarch64|x86_64`,
      );
    }
    selected.push({ key: fallback, file: generic[0] });
  } else if (generic.length > 1) {
    if (!fallback) {
      throw new Error(
        `cannot determine architecture for ${generic.length} generic macOS artifacts; pass --mac-arch aarch64|x86_64`,
      );
    } else {
      // A feed has one artifact per platform key. Keep the first deterministic
      // generic artifact and make the ambiguity visible to release operators.
      generic.sort();
      selected.push({ key: fallback, file: generic[0] });
      for (const file of generic.slice(1)) {
        console.warn(`skip ${path.basename(file)}: duplicate generic artifact for ${fallback}`);
      }
    }
  }

  // De-duplicate a key deterministically; this also prevents an arm artifact
  // from being overwritten by a later generic file.
  const byKey = new Map();
  // Explicit filename markers always win over a generic fallback for the
  // same key, regardless of directory enumeration order.
  for (const item of selected
    .filter(({ file }) => inferMacArch(path.basename(file)))
    .sort((a, b) => a.file.localeCompare(b.file))) {
    if (!byKey.has(item.key)) byKey.set(item.key, item.file);
  }
  for (const item of selected
    .filter(({ file }) => !inferMacArch(path.basename(file)))
    .sort((a, b) => a.file.localeCompare(b.file))) {
    if (!byKey.has(item.key)) byKey.set(item.key, item.file);
  }
  return [...byKey.entries()].map(([key, file]) => ({ key, file }));
}

function buildFeed(args) {
  if (!args.version || !args.baseUrl) {
    throw new Error('Required: --version and --base-url');
  }

  const platforms = {};
  const nsis = path.join(args.targetDir, 'nsis');
  const msi = path.join(args.targetDir, 'msi');
  const appimage = path.join(args.targetDir, 'appimage');
  const macos = path.join(args.targetDir, 'macos');

  // Prefer NSIS setup for Windows x64 updater install.
  const nsisExe = findFirst(nsis, (n) => n.endsWith('-setup.exe') || n.endsWith('.exe'));
  addPlatform(platforms, 'windows-x86_64', nsisExe, args.baseUrl);

  if (!platforms['windows-x86_64']) {
    const msiFile = findFirst(msi, (n) => n.endsWith('.msi'));
    addPlatform(platforms, 'windows-x86_64', msiFile, args.baseUrl);
  }

  const appImg = findFirst(appimage, (n) => n.endsWith('.AppImage'));
  addPlatform(platforms, 'linux-x86_64', appImg, args.baseUrl);

  for (const { key, file } of macArtifacts(macos, args.macArch)) {
    addPlatform(platforms, key, file, args.baseUrl);
  }

  if (Object.keys(platforms).length === 0) {
    throw new Error(`No signed updater artifacts under ${args.targetDir}`);
  }

  return {
    version: args.version.replace(/^v/, ''),
    notes: args.notes,
    pub_date: new Date().toISOString(),
    platforms,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  let feed;
  try {
    feed = buildFeed(args);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
  const outPath = path.resolve(args.out);
  fs.writeFileSync(outPath, `${JSON.stringify(feed, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outPath}`);
}

export { addPlatform, buildFeed, inferMacArch, macArtifacts, normalizeMacArch, parseArgs };

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) main();
