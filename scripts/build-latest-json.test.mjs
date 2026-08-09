import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { buildFeed, macArtifacts, normalizeMacArch } from './build-latest-json.mjs';

function tempBundle() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-latest-json-'));
  const macos = path.join(root, 'macos');
  const nsis = path.join(root, 'nsis');
  const msi = path.join(root, 'msi');
  fs.mkdirSync(macos, { recursive: true });
  fs.mkdirSync(nsis, { recursive: true });
  fs.mkdirSync(msi, { recursive: true });
  return { root, macos, nsis, msi };
}

function signedArtifact(dir, name, signature = 'sig') {
  const artifact = path.join(dir, name);
  fs.writeFileSync(artifact, 'artifact');
  fs.writeFileSync(`${artifact}.sig`, `${signature}\n`);
}

test('macOS artifacts are keyed from filename architecture markers', () => {
  const { macos } = tempBundle();
  signedArtifact(macos, 'AgentHub_1.0.0_aarch64.app.tar.gz', 'arm-sig');
  signedArtifact(macos, 'AgentHub_1.0.0_x86_64.app.tar.gz', 'intel-sig');

  assert.deepEqual(
    macArtifacts(macos).map(({ key }) => key).sort(),
    ['darwin-aarch64', 'darwin-x86_64'],
  );
  const feed = buildFeed({
    version: 'v1.0.0',
    notes: '',
    baseUrl: 'https://example.invalid/release',
    out: 'latest.json',
    targetDir: path.dirname(macos),
    macArch: null,
  });
  assert.equal(feed.platforms['darwin-aarch64'].signature, 'arm-sig');
  assert.equal(feed.platforms['darwin-x86_64'].signature, 'intel-sig');
});

test('generic macOS artifact accepts explicit --mac-arch', () => {
  const { macos } = tempBundle();
  signedArtifact(macos, 'AgentHub_1.0.0.app.tar.gz');
  assert.equal(normalizeMacArch('arm64'), 'darwin-aarch64');
  assert.deepEqual(macArtifacts(macos, 'darwin-aarch64'), [
    { key: 'darwin-aarch64', file: path.join(macos, 'AgentHub_1.0.0.app.tar.gz') },
  ]);
});

test('generic macOS artifact fails closed without explicit architecture', () => {
  const { macos } = tempBundle();
  signedArtifact(macos, 'AgentHub_1.0.0.app.tar.gz');
  assert.throws(
    () => macArtifacts(macos),
    /cannot determine architecture.*--mac-arch/,
  );
});

test('builds Windows and ARM macOS updater feed from downloaded artifact layout', () => {
  const { root, macos, nsis, msi } = tempBundle();
  signedArtifact(nsis, 'AgentHub_1.0.0_x64-setup.exe', 'windows-sig');
  signedArtifact(msi, 'AgentHub_1.0.0_x64_en-US.msi', 'msi-sig');
  signedArtifact(macos, 'AgentHub_1.0.0_aarch64.app.tar.gz', 'mac-sig');

  const feed = buildFeed({
    version: '1.0.0',
    notes: 'release notes',
    baseUrl: 'https://example.invalid/releases/download/v1.0.0',
    out: 'latest.json',
    targetDir: root,
    macArch: 'aarch64',
  });

  assert.equal(feed.platforms['windows-x86_64'].signature, 'windows-sig');
  assert.equal(feed.platforms['darwin-aarch64'].signature, 'mac-sig');
});

test('fails when a selected updater signature is empty after trimming', () => {
  const { root, nsis } = tempBundle();
  signedArtifact(nsis, 'AgentHub_1.0.0_x64-setup.exe', ' \n\t ');
  assert.throws(
    () => buildFeed({
      version: '1.0.0',
      notes: '',
      baseUrl: 'https://example.invalid/releases/download/v1.0.0',
      out: 'latest.json',
      targetDir: root,
      macArch: null,
    }),
    /empty updater signature/,
  );
});
