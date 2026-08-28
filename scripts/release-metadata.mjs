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
const WORKSPACE_LOCK_PACKAGES = ['agenthub-cli', 'agenthub-core', 'agenthub-gui'];

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

/** Resolve tauri.conf.json version: literal semver, ../package.json path, or Cargo fallback. */
function readTauriConfigVersion(root = defaultRoot) {
  const tauriPath = path.join(root, 'src-tauri', 'tauri.conf.json');
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));
  } catch (error) {
    throw new Error(
      `Unable to read src-tauri/tauri.conf.json: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const raw = parsed.version;
  if (raw == null || raw === '') {
    const cargoContents = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
    return readCargoWorkspaceVersion(cargoContents);
  }
  if (typeof raw !== 'string') {
    throw new Error('src-tauri/tauri.conf.json version must be a string, package.json path, or omitted');
  }
  if (/package\.json$/i.test(raw.trim())) {
    const packagePath = path.resolve(path.dirname(tauriPath), raw);
    return readJsonVersion(packagePath, `tauri.conf.json version path (${raw})`);
  }
  return raw;
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
  const expected = WORKSPACE_LOCK_PACKAGES;
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

const CHANGELOG_PATH = 'CHANGELOG.md';
const CHANGELOG_SECTION =
  /^##\s+\[?(?<version>(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?)\]?(?:\s+-\s+\d{4}-\d{2}-\d{2})?\s*$/;

function readChangelogContents(root = defaultRoot) {
  const changelogPath = path.join(root, CHANGELOG_PATH);
  try {
    return fs.readFileSync(changelogPath, 'utf8');
  } catch (error) {
    throw new Error(
      `Missing ${CHANGELOG_PATH}. Add a release notes section for this version before publishing.`,
    );
  }
}

function extractChangelogSection(content, version) {
  const lines = content.split(/\r?\n/);
  let start = -1;
  let inFence = false;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^```/.test(line.trim())) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

    const match = line.match(CHANGELOG_SECTION);
    if (match?.groups?.version === version) {
      start = index + 1;
      break;
    }
  }
  if (start === -1) {
    return null;
  }

  const body = [];
  inFence = false;
  for (let index = start; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^```/.test(line.trim())) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (/^##\s+/.test(line)) break;
    body.push(line);
  }

  while (body.length > 0 && body[0].trim() === '') body.shift();
  while (body.length > 0 && body[body.length - 1].trim() === '') body.pop();
  return body.join('\n');
}

function assertReleaseChangelog(root, version) {
  const section = extractChangelogSection(readChangelogContents(root), version);
  if (section == null) {
    throw new Error(
      `${CHANGELOG_PATH} is missing a release section for version ${version}. Use a heading like '## [${version}] - YYYY-MM-DD'.`,
    );
  }
  const hasBullet = section.split(/\r?\n/).some((line) => /^\s*[-*]\s+\S/.test(line));
  if (!hasBullet) {
    throw new Error(
      `${CHANGELOG_PATH} section for version ${version} must include at least one '- ' release note bullet.`,
    );
  }
  return section;
}

function readReleaseNotesForVersion(root, version) {
  return assertReleaseChangelog(root, version);
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
  const tauriVersion = readTauriConfigVersion(root);

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

/** Propagate package.json version into Cargo.toml and Cargo.lock workspace entries. */
function syncReleaseVersionFromPackageJson(root = defaultRoot, explicitVersion) {
  const packagePath = path.join(root, 'package.json');
  const version = explicitVersion ?? readJsonVersion(packagePath, 'package.json');
  assertStrictSemVer(version, 'package.json');

  const cargoPath = path.join(root, 'Cargo.toml');
  let cargoContents = fs.readFileSync(cargoPath, 'utf8');
  const currentCargoVersion = readCargoWorkspaceVersion(cargoContents);
  if (currentCargoVersion !== version) {
    const cargoUpdated = cargoContents.replace(
      /(\[workspace\.package\][\s\S]*?version\s*=\s*")[^"]+(")/m,
      `$1${version}$2`,
    );
    if (cargoUpdated === cargoContents) {
      throw new Error('Failed to patch Cargo.toml [workspace.package].version');
    }
    fs.writeFileSync(cargoPath, cargoUpdated);
    cargoContents = cargoUpdated;
  }

  const lockPath = path.join(root, 'Cargo.lock');
  let lockContents = fs.readFileSync(lockPath, 'utf8');
  const lockVersions = readCargoLockWorkspaceVersions(lockContents);
  const lockMismatch = WORKSPACE_LOCK_PACKAGES.some((name) => lockVersions[name] !== version);
  if (lockMismatch) {
    for (const name of WORKSPACE_LOCK_PACKAGES) {
      const pattern = new RegExp(
        `(\\[\\[package\\]\\]\\s*name\\s*=\\s*"${name}"\\s*version\\s*=\\s*")[^"]+(")`,
        'm',
      );
      if (!pattern.test(lockContents)) {
        throw new Error(`Cargo.lock must contain exactly one workspace package entry for ${name}`);
      }
      lockContents = lockContents.replace(pattern, `$1${version}$2`);
    }
    fs.writeFileSync(lockPath, lockContents);
  }

  return readReleaseMetadata(root);
}

function assertTagMatchesMetadata(metadata, gitTag) {
  if (typeof gitTag !== 'string' || gitTag.length === 0) {
    throw new Error('Git tag is required');
  }
  if (gitTag !== metadata.tag) {
    throw new Error(`Git tag '${gitTag}' does not match release metadata tag '${metadata.tag}'`);
  }
}

function parseCliArgs(argv) {
  const options = {
    root: defaultRoot,
    githubOutput: null,
    expectTag: null,
    requireChangelog: false,
    notesOut: null,
  };
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
    } else if (argument === '--expect-tag') {
      const value = argv[++index];
      if (!value) throw new Error('--expect-tag requires a tag name');
      options.expectTag = value;
    } else if (argument === '--require-changelog') {
      options.requireChangelog = true;
    } else if (argument === '--notes-out') {
      const value = argv[++index];
      if (!value) throw new Error('--notes-out requires a file path');
      options.notesOut = path.resolve(value);
    } else if (argument === '--help' || argument === '-h') {
      console.log(
        'Usage: node scripts/release-metadata.mjs [--root DIR] [--github-output FILE] [--expect-tag TAG] [--require-changelog] [--notes-out FILE]',
      );
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
  if (options.expectTag) {
    assertTagMatchesMetadata(metadata, options.expectTag);
  }
  let releaseNotes = null;
  if (options.requireChangelog || options.expectTag || options.notesOut) {
    releaseNotes = readReleaseNotesForVersion(options.root, metadata.version);
  }
  if (options.notesOut && releaseNotes != null) {
    fs.writeFileSync(options.notesOut, `${releaseNotes}\n`, 'utf8');
  }
  if (options.githubOutput) {
    fs.appendFileSync(
      options.githubOutput,
      `version=${metadata.version}\ntag=${metadata.tag}\nprerelease=${metadata.prerelease}\n`,
      'utf8',
    );
  }
  console.log(JSON.stringify({ ...metadata, releaseNotes }));
}

export {
  STRICT_SEMVER,
  assertReleaseChangelog,
  assertStrictSemVer,
  assertTagMatchesMetadata,
  extractChangelogSection,
  isPrerelease,
  parseCliArgs,
  readCargoLockWorkspaceVersions,
  readCargoWorkspaceVersion,
  readReleaseMetadata,
  readReleaseNotesForVersion,
  readTauriConfigVersion,
  syncReleaseVersionFromPackageJson,
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
