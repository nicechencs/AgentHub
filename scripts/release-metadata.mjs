#!/usr/bin/env node
/**
 * Validate the version that a release workflow publishes.
 *
 * A desktop release is only safe when the JavaScript package, Cargo workspace,
 * and Tauri bundle all describe precisely the same strict SemVer version.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const defaultRoot = path.resolve(__dirname, '..');

// SemVer 2.0.0, intentionally excluding a leading `v` and numeric leading
// zeroes. Prerelease and build identifiers remain fully supported.
const STRICT_SEMVER = /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function readJsonVersion(filePath, label) {
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`Unable to read ${label}: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (typeof parsed.version !== 'string') {
    throw new Error(`${label} must contain a string version`);
  }
  return parsed.version;
}

/** Read only [workspace.package].version, never an unrelated Cargo section. */
function readCargoWorkspaceVersion(cargoToml) {
  let inWorkspacePackage = false;
  let foundWorkspacePackage = false;
  const versions = [];

  for (const line of cargoToml.split(/\r?\n/)) {
    const section = line.match(/^\s*\[([^\]]+)\]\s*(?:#.*)?$/);
    if (section) {
      inWorkspacePackage = section[1].trim() === 'workspace.package';
      foundWorkspacePackage ||= inWorkspacePackage;
      continue;
    }
    if (!inWorkspacePackage) continue;

    const version = line.match(/^\s*version\s*=\s*(["'])(.*?)\1\s*(?:#.*)?$/);
    if (version) versions.push(version[2]);
  }

  if (!foundWorkspacePackage) {
    throw new Error('Cargo.toml is missing [workspace.package]');
  }
  if (versions.length !== 1) {
    throw new Error('Cargo.toml [workspace.package] must contain exactly one string version');
  }
  return versions[0];
}

/** Read versions for the three local workspace packages from Cargo.lock. */
function readCargoLockWorkspaceVersions(cargoLock) {
  const expected = ['agenthub-cli', 'agenthub-core', 'agenthub-gui'];
  const versions = new Map();
  let packageName = null;

  for (const line of cargoLock.split(/\r?\n/)) {
    if (/^\s*\[\[package\]\]\s*$/.test(line)) {
      packageName = null;
      continue;
    }
    const name = line.match(/^\s*name\s*=\s*"([^"]+)"\s*$/);
    if (name) {
      packageName = expected.includes(name[1]) ? name[1] : null;
      continue;
    }
    if (packageName) {
      const version = line.match(/^\s*version\s*=\s*"([^"]+)"\s*$/);
      if (version) {
        if (versions.has(packageName)) {
          throw new Error(`Cargo.lock contains duplicate workspace package entry: ${packageName}`);
        }
        versions.set(packageName, version[1]);
      }
    }
  }

  const missing = expected.filter((name) => !versions.has(name));
  if (missing.length > 0) {
    throw new Error(`Cargo.lock is missing workspace package entries: ${missing.join(', ')}`);
  }
  return Object.fromEntries(expected.map((name) => [name, versions.get(name)]));
}

function assertStrictSemVer(version, source) {
  if (!STRICT_SEMVER.test(version)) {
    throw new Error(`${source} version '${version}' is not strict SemVer (use X.Y.Z without a leading v)`);
  }
}

function isPrerelease(version) {
  // SemVer build metadata begins at `+`; a hyphen there is not a prerelease.
  return version.split('+', 1)[0].includes('-');
}

function readReleaseMetadata(root = defaultRoot) {
  const packageVersion = readJsonVersion(path.join(root, 'package.json'), 'package.json');
  const cargoPath = path.join(root, 'Cargo.toml');
  let cargoContents;
  try {
    cargoContents = fs.readFileSync(cargoPath, 'utf8');
  } catch (error) {
    throw new Error(`Unable to read Cargo.toml: ${error instanceof Error ? error.message : String(error)}`);
  }
  const cargoVersion = readCargoWorkspaceVersion(cargoContents);
  let cargoLockContents;
  try {
    cargoLockContents = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8');
  } catch (error) {
    throw new Error(`Unable to read Cargo.lock: ${error instanceof Error ? error.message : String(error)}`);
  }
  const cargoLockVersions = readCargoLockWorkspaceVersions(cargoLockContents);
  const tauriVersion = readJsonVersion(path.join(root, 'src-tauri', 'tauri.conf.json'), 'src-tauri/tauri.conf.json');

  for (const [source, version] of [
    ['package.json', packageVersion],
    ['Cargo.toml [workspace.package]', cargoVersion],
    ['src-tauri/tauri.conf.json', tauriVersion],
  ]) {
    assertStrictSemVer(version, source);
  }

  if (packageVersion !== cargoVersion || packageVersion !== tauriVersion) {
    throw new Error(
      `Release versions must match: package.json=${packageVersion}, Cargo.toml [workspace.package]=${cargoVersion}, src-tauri/tauri.conf.json=${tauriVersion}`,
    );
  }

  const lockMismatches = Object.entries(cargoLockVersions)
    .filter(([, version]) => version !== packageVersion)
    .map(([name, version]) => `${name}=${version}`);
  if (lockMismatches.length > 0) {
    throw new Error(
      `Cargo.lock workspace package versions must match: expected=${packageVersion}, ${lockMismatches.join(', ')}`,
    );
  }

  return {
    version: packageVersion,
    tag: `v${packageVersion}`,
    prerelease: isPrerelease(packageVersion),
  };
}

function parseCliArgs(argv) {
  const options = { root: defaultRoot, githubOutput: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--root') {
      const value = argv[++index];
      if (!value) throw new Error('--root requires a directory');
      options.root = path.resolve(value);
    } else if (argument === '--github-output') {
      const value = argv[++index];
      if (!value) throw new Error('--github-output requires a file path');
      options.githubOutput = path.resolve(value);
    } else if (argument === '--help' || argument === '-h') {
      console.log('Usage: node scripts/release-metadata.mjs [--root DIR] [--github-output FILE]');
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return options;
}

function main() {
  const options = parseCliArgs(process.argv.slice(2));
  const metadata = readReleaseMetadata(options.root);
  if (options.githubOutput) {
    fs.appendFileSync(
      options.githubOutput,
      `version=${metadata.version}\ntag=${metadata.tag}\nprerelease=${metadata.prerelease}\n`,
      'utf8',
    );
  }
  console.log(JSON.stringify(metadata));
}

export {
  STRICT_SEMVER,
  assertStrictSemVer,
  isPrerelease,
  parseCliArgs,
  readCargoLockWorkspaceVersions,
  readCargoWorkspaceVersion,
  readReleaseMetadata,
};

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
