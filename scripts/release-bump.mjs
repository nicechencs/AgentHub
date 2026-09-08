#!/usr/bin/env node
/**
 * Cross-platform version bump used by `pnpm release:bump`.
 *
 * Writes package.json then syncs Cargo.toml / Cargo.lock. Does not build
 * installers; GitHub Actions publishes those from a v* tag.
 */
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

import {
  bumpSemVer,
  nextFreeReleaseVersion,
  readReleaseMetadata,
  syncReleaseVersionFromPackageJson,
} from './release-metadata.mjs';

const defaultRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function parseArgs(argv) {
  const options = {
    root: defaultRoot,
    kind: null,
    skipRemote: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--root') {
      options.root = argv[++index];
      if (!options.root) throw new Error('--root requires a directory');
    } else if (argument === '--patch' || argument === '--minor' || argument === '--major') {
      const kind = argument.slice(2);
      if (options.kind && options.kind !== kind) {
        throw new Error('Use only one of --patch / --minor / --major');
      }
      options.kind = kind;
    } else if (argument === '--skip-remote') {
      options.skipRemote = true;
    } else if (argument === '--help' || argument === '-h') {
      console.log(
        'Usage: node scripts/release-bump.mjs [--patch|--minor|--major] [--root DIR] [--skip-remote]',
      );
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  options.kind = options.kind ?? 'patch';
  return options;
}

function readPackageVersion(root) {
  const packagePath = path.join(root, 'package.json');
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  } catch (error) {
    throw new Error(
      `Unable to read package.json: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (typeof parsed.version !== 'string') {
    throw new Error('package.json must contain a string version');
  }
  return parsed.version;
}

function writePackageVersion(root, version) {
  const packagePath = path.join(root, 'package.json');
  const text = fs.readFileSync(packagePath, 'utf8');
  const updated = text.replace(/("version"\s*:\s*")[^"]+(")/, `$1${version}$2`);
  if (updated === text) {
    throw new Error('Failed to patch package.json version');
  }
  fs.writeFileSync(packagePath, updated);
}

function gitRemoteTagTaken(root, tag) {
  const result = spawnSync('git', ['ls-remote', '--refs', 'origin', `refs/tags/${tag}`], {
    cwd: root,
    encoding: 'utf8',
  });
  if (result.error) {
    throw new Error(
      `Unable to query remote tag ${tag} via git ls-remote: ${result.error.message}`,
    );
  }
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || '').trim();
    throw new Error(
      `Unable to query remote tag ${tag} via git ls-remote; refuse to auto-bump without a definitive result.${
        detail ? ` ${detail}` : ''
      }`,
    );
  }
  return result.stdout.trim().length > 0;
}

function bumpReleaseVersion(options) {
  const root = path.resolve(options.root);
  const current = readPackageVersion(root);
  const computed = bumpSemVer(current, options.kind);
  const version = options.skipRemote
    ? computed
    : nextFreeReleaseVersion(computed, (tag) => gitRemoteTagTaken(root, tag));
  writePackageVersion(root, version);
  syncReleaseVersionFromPackageJson(root);
  return { ...readReleaseMetadata(root), previous: current };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const result = bumpReleaseVersion(options);
  console.log(
    JSON.stringify({
      previous: result.previous,
      version: result.version,
      tag: result.tag,
      prerelease: result.prerelease,
    }),
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(process.argv[1]).href : null;
if (invokedPath === import.meta.url) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

export { bumpReleaseVersion, parseArgs, writePackageVersion };
