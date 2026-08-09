import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { buildFeed, macArtifacts, normalizeMacArch } from './build-latest-json.mjs';

function tempBundle() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-latest-json-'));
  const macos = path.join(root, 'macos');
  fs.mkdirSync(macos, { recursive: true });
  return { root, macos };
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
