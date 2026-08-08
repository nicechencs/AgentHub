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
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

function parseArgs(argv) {
  const out = {
    version: null,
    notes: '',
    baseUrl: null,
    out: 'latest.json',
    targetDir: path.join(root, 'target', 'release', 'bundle'),
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--version') out.version = argv[++i];
    else if (a === '--notes') out.notes = argv[++i] ?? '';
    else if (a === '--base-url') out.baseUrl = argv[++i];
    else if (a === '--out') out.out = argv[++i];
    else if (a === '--target-dir') out.targetDir = path.resolve(argv[++i]);
    else if (a === '--help' || a === '-h') {
      console.log(`Usage: node scripts/build-latest-json.mjs --version X.Y.Z --base-url URL [--notes TEXT] [--out latest.json]`);
      process.exit(0);
    }
  }
  return out;
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

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.version || !args.baseUrl) {
    console.error('Required: --version and --base-url');
    process.exit(1);
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

  const macTar = findFirst(macos, (n) => n.endsWith('.app.tar.gz'));
  // Host arch of the build machine is unknown; writers can rename keys later.
  // Emit as darwin-x86_64 when only one tarball is present (common for CI matrix).
  addPlatform(platforms, 'darwin-x86_64', macTar, args.baseUrl);
  // aarch64 builds typically produce the same filename pattern in a separate job.
  const macTarAarch = findFirst(macos, (n) => n.includes('aarch64') && n.endsWith('.app.tar.gz'));
  if (macTarAarch && macTarAarch !== macTar) {
    addPlatform(platforms, 'darwin-aarch64', macTarAarch, args.baseUrl);
  }

  if (Object.keys(platforms).length === 0) {
    console.error(`No signed updater artifacts under ${args.targetDir}`);
    process.exit(1);
  }

  const feed = {
    version: args.version.replace(/^v/, ''),
    notes: args.notes,
    pub_date: new Date().toISOString(),
    platforms,
  };

  const outPath = path.resolve(args.out);
  fs.writeFileSync(outPath, `${JSON.stringify(feed, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outPath}`);
}

main();
