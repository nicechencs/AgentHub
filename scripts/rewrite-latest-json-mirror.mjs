#!/usr/bin/env node
/**
 * Rewrite Tauri latest.json platform download URLs to a mirror base URL.
 *
 * Keeps filenames; replaces the origin/path prefix so installers are fetched
 * from the domestic mirror instead of GitHub.
 *
 * Usage:
 *   node scripts/rewrite-latest-json-mirror.mjs \
 *     --in release-assets/latest.json \
 *     --out release-assets/latest.mirror.json \
 *     --mirror-base-url https://updates.agenthub.qooo.io
 *
 * Or rewrite in place:
 *   node scripts/rewrite-latest-json-mirror.mjs \
 *     --in latest.json --out latest.json \
 *     --mirror-base-url https://updates.agenthub.qooo.io
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

function parseArgs(argv) {
  const out = {
    inPath: null,
    outPath: null,
    mirrorBaseUrl: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--in') out.inPath = argv[++i];
    else if (a === '--out') out.outPath = argv[++i];
    else if (a === '--mirror-base-url') out.mirrorBaseUrl = argv[++i];
    else if (a === '--help' || a === '-h') {
      console.log(
        'Usage: node scripts/rewrite-latest-json-mirror.mjs --in latest.json --out latest.json --mirror-base-url URL',
      );
      process.exit(0);
    }
  }
  return out;
}

function normalizeMirrorBase(url) {
  const raw = String(url ?? '').trim();
  if (!raw) {
    throw new Error('--mirror-base-url is required');
  }
  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    throw new Error(`Invalid --mirror-base-url: ${raw}`);
  }
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
    throw new Error(`--mirror-base-url must be http(s); got ${parsed.protocol}`);
  }
  // Strip trailing slash and any path so join is always origin + /filename
  return `${parsed.origin}${parsed.pathname.replace(/\/+$/, '')}`.replace(/\/$/, '') || parsed.origin;
}

function filenameFromUrl(url) {
  const text = String(url ?? '');
  let pathname;
  try {
    pathname = new URL(text).pathname;
  } catch {
    pathname = text;
  }
  const name = path.posix.basename(pathname);
  if (!name || name === '/' || name === '.') {
    throw new Error(`Cannot extract filename from url: ${text}`);
  }
  return name;
}

/**
 * @param {object} feed
 * @param {string} mirrorBaseUrl
 * @returns {object}
 */
function rewriteLatestJson(feed, mirrorBaseUrl) {
  if (!feed || typeof feed !== 'object' || Array.isArray(feed)) {
    throw new Error('latest.json root must be an object');
  }
  const base = normalizeMirrorBase(mirrorBaseUrl);
  const platforms = feed.platforms;
  if (!platforms || typeof platforms !== 'object' || Array.isArray(platforms)) {
    throw new Error('latest.json is missing platforms object');
  }

  const nextPlatforms = {};
  for (const [key, entry] of Object.entries(platforms)) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new Error(`platforms.${key} must be an object`);
    }
    if (typeof entry.url !== 'string' || !entry.url) {
      throw new Error(`platforms.${key}.url is required`);
    }
    const fileName = filenameFromUrl(entry.url);
    nextPlatforms[key] = {
      ...entry,
      url: `${base}/${fileName}`,
    };
  }

  return {
    ...feed,
    platforms: nextPlatforms,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.inPath || !args.outPath || !args.mirrorBaseUrl) {
    throw new Error('Required: --in, --out, --mirror-base-url');
  }
  const inPath = path.resolve(args.inPath);
  const outPath = path.resolve(args.outPath);
  const feed = JSON.parse(fs.readFileSync(inPath, 'utf8'));
  const rewritten = rewriteLatestJson(feed, args.mirrorBaseUrl);
  fs.writeFileSync(outPath, `${JSON.stringify(rewritten, null, 2)}\n`, 'utf8');
  console.log(`Wrote mirrored feed ${outPath}`);
}

export { filenameFromUrl, normalizeMirrorBase, parseArgs, rewriteLatestJson };

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
