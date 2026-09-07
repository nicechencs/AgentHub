import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { bumpReleaseVersion } from './release-bump.mjs';
import {
  bumpSemVer,
  nextFreeReleaseVersion,
  readCargoLockWorkspaceVersions,
  readCargoWorkspaceVersion,
} from './release-metadata.mjs';

function writeBumpFixture(packageVersion = '1.2.3') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-release-bump-'));
  fs.mkdirSync(path.join(root, 'src-tauri'));
  fs.writeFileSync(
    path.join(root, 'package.json'),
    `{\n  "name": "fixture",\n  "version": "${packageVersion}"\n}\n`,
  );
  fs.writeFileSync(
    path.join(root, 'Cargo.toml'),
    `[workspace]\nresolver = "2"\n\n[workspace.package]\nversion = "${packageVersion}"\n`,
  );
  fs.writeFileSync(
    path.join(root, 'Cargo.lock'),
    [
      `[[package]]\nname = "agenthub-cli"\nversion = "${packageVersion}"`,
      `[[package]]\nname = "agenthub-core"\nversion = "${packageVersion}"`,
      `[[package]]\nname = "agenthub-gui"\nversion = "${packageVersion}"`,
    ].join('\n\n') + '\n',
  );
  fs.writeFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), '{"version":"../package.json"}\n');
  return root;
}

test('bumpSemVer matches patch/minor/major and drops prerelease', () => {
  assert.equal(bumpSemVer('0.4.10', 'patch'), '0.4.11');
  assert.equal(bumpSemVer('0.4.10', 'minor'), '0.5.0');
  assert.equal(bumpSemVer('0.4.10', 'major'), '1.0.0');
  assert.equal(bumpSemVer('1.2.3-rc.1+build.8', 'patch'), '1.2.4');
});

test('nextFreeReleaseVersion skips occupied tags then returns the first free patch', () => {
  const taken = new Set(['v1.2.4', 'v1.2.5']);
  assert.equal(
    nextFreeReleaseVersion('1.2.4', (tag) => taken.has(tag)),
    '1.2.6',
  );
});

test('nextFreeReleaseVersion fails closed when every attempt is taken', () => {
  assert.throws(
    () => nextFreeReleaseVersion('1.0.0', () => true, 3),
    /after 3 attempts/,
  );
});

test('bumpReleaseVersion writes package.json and syncs cargo files', () => {
  const root = writeBumpFixture('2.0.0');
  try {
    const result = bumpReleaseVersion({ root, kind: 'minor', skipRemote: true });
    assert.deepEqual(result, {
      previous: '2.0.0',
      version: '2.1.0',
      tag: 'v2.1.0',
      prerelease: false,
    });
    assert.match(fs.readFileSync(path.join(root, 'package.json'), 'utf8'), /"version": "2.1.0"/);
    assert.equal(
      readCargoWorkspaceVersion(fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8')),
      '2.1.0',
    );
    assert.deepEqual(readCargoLockWorkspaceVersions(fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8')), {
      'agenthub-cli': '2.1.0',
      'agenthub-core': '2.1.0',
      'agenthub-gui': '2.1.0',
    });
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
