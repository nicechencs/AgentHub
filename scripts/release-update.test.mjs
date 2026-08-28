import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const scriptsDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptsDirectory, '..');
const powershell = process.platform === 'win32' ? 'powershell.exe' : 'pwsh';

function commandAvailable(command) {
  try {
    const result = spawnSync(command, ['-NoProfile', '-Command', 'exit 0'], { stdio: 'ignore' });
    return result.error == null && result.status === 0;
  } catch {
    return false;
  }
}

function createFixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-release-update-'));
  const origin = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-release-origin-'));
  fs.mkdirSync(path.join(root, 'scripts'));
  fs.mkdirSync(path.join(root, 'src-tauri'));
  fs.copyFileSync(path.join(repositoryRoot, 'scripts', 'release-update.ps1'), path.join(root, 'scripts', 'release-update.ps1'));
  fs.copyFileSync(path.join(repositoryRoot, 'scripts', 'release-metadata.mjs'), path.join(root, 'scripts', 'release-metadata.mjs'));
  fs.writeFileSync(path.join(root, 'package.json'), '{"name":"fixture","version":"1.2.3"}\n');
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[workspace]\nresolver = "2"\n\n[workspace.package]\nversion = "1.2.3"\n');
  fs.writeFileSync(
    path.join(root, 'Cargo.lock'),
    [
      '[[package]]\nname = "agenthub-cli"\nversion = "1.2.3"',
      '[[package]]\nname = "agenthub-core"\nversion = "1.2.3"',
      '[[package]]\nname = "agenthub-gui"\nversion = "1.2.3"',
    ].join('\n\n') + '\n',
  );
  fs.writeFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), '{"version":"1.2.3"}\n');

  assert.equal(spawnSync('git', ['init', '--bare', origin], { stdio: 'ignore' }).status, 0);
  assert.equal(spawnSync('git', ['init', root], { stdio: 'ignore' }).status, 0);
  assert.equal(spawnSync('git', ['-C', root, 'remote', 'add', 'origin', origin], { stdio: 'ignore' }).status, 0);
  return { root, origin };
}

function snapshotFiles(root) {
  return new Map(
    ['package.json', 'Cargo.toml', 'Cargo.lock', path.join('src-tauri', 'tauri.conf.json')].map((relativePath) => [
      relativePath,
      fs.readFileSync(path.join(root, relativePath)),
    ]),
  );
}

function runUpdate(root, environment = {}) {
  return spawnSync(
    powershell,
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      path.join(root, 'scripts', 'release-update.ps1'),
      '-Version',
      '9.9.9',
      '-Bump',
      '-VersionOnly',
    ],
    {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, ...environment },
    },
  );
}

function assertSnapshotUnchanged(root, snapshot) {
  for (const [relativePath, before] of snapshot) {
    assert.deepEqual(fs.readFileSync(path.join(root, relativePath)), before, relativePath);
  }
}

test('release version replacement rolls back every file when an intermediate replace fails', { skip: !commandAvailable(powershell) }, () => {
  const { root, origin } = createFixture();
  try {
    const before = snapshotFiles(root);
    const result = runUpdate(root, { AGENTHUB_RELEASE_FAIL_REPLACE_AT: '2' });
    assert.notEqual(result.status, 0, result.stdout + result.stderr);
    assertSnapshotUnchanged(root, before);
    assert.equal(fs.readdirSync(path.dirname(path.join(root, 'package.json'))).some((name) => name.includes('.agenthub-')), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(origin, { recursive: true, force: true });
  }
});

test('release version replacement rolls back every file when post-check fails', { skip: !commandAvailable(powershell) }, () => {
  const { root, origin } = createFixture();
  try {
    const before = snapshotFiles(root);
    const result = runUpdate(root, { AGENTHUB_RELEASE_FAIL_POSTCHECK: '1' });
    assert.notEqual(result.status, 0, result.stdout + result.stderr);
    assertSnapshotUnchanged(root, before);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(origin, { recursive: true, force: true });
  }
});

test('release version replacement retains the backup when rollback itself fails', { skip: !commandAvailable(powershell) }, () => {
  const { root, origin } = createFixture();
  try {
    const result = runUpdate(root, {
      AGENTHUB_RELEASE_FAIL_POSTCHECK: '1',
      AGENTHUB_RELEASE_FAIL_ROLLBACK_AT: '1',
    });
    const output = `${result.stdout}\n${result.stderr}`;
    assert.notEqual(result.status, 0, output);
    assert.match(output, /Release version rollback failed/);
    const backupName = fs.readdirSync(root).find((name) => name.endsWith('.bak'));
    assert.ok(backupName);
    const absoluteBackup = path.resolve(root, backupName);
    const escapedBackup = absoluteBackup.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    assert.match(output, new RegExp(`backup retained at ${escapedBackup}`));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(origin, { recursive: true, force: true });
  }
});

test('release version replacement commits all four files without a UTF-8 BOM', { skip: !commandAvailable(powershell) }, () => {
  const { root, origin } = createFixture();
  try {
    const result = runUpdate(root);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    for (const relativePath of ['package.json', 'Cargo.toml', 'Cargo.lock', path.join('src-tauri', 'tauri.conf.json')]) {
      const bytes = fs.readFileSync(path.join(root, relativePath));
      assert.notDeepEqual(bytes.subarray(0, 3), Buffer.from([0xef, 0xbb, 0xbf]), relativePath);
    }
    assert.equal(JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version, '9.9.9');
    assert.match(fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8'), /version = "9\.9\.9"/);
    assert.match(fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8'), /name = "agenthub-gui"\nversion = "9\.9\.9"/);
    assert.equal(JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version, '../package.json');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(origin, { recursive: true, force: true });
  }
});
