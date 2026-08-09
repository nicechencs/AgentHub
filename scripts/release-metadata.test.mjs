import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { isPrerelease, readCargoWorkspaceVersion, readReleaseMetadata } from './release-metadata.mjs';

function writeReleaseFixture({ packageVersion = '1.2.3', cargoVersion = packageVersion, tauriVersion = packageVersion } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-release-metadata-'));
  fs.mkdirSync(path.join(root, 'src-tauri'));
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ version: packageVersion }));
  fs.writeFileSync(
    path.join(root, 'Cargo.toml'),
    `[workspace]\nresolver = "2"\nversion = "99.99.99"\n\n[workspace.package]\nversion = "${cargoVersion}"\n\n[workspace.dependencies]\nserde = "1"\n`,
  );
  fs.writeFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), JSON.stringify({ version: tauriVersion }));
  return root;
}

test('returns one metadata value when all release versions agree', () => {
  const root = writeReleaseFixture({ packageVersion: '1.2.3-rc.1+build.8' });
  assert.deepEqual(readReleaseMetadata(root), {
    version: '1.2.3-rc.1+build.8',
    tag: 'v1.2.3-rc.1+build.8',
    prerelease: true,
  });
});

test('identifies only SemVer prerelease components as prereleases', () => {
  assert.equal(isPrerelease('1.2.3-rc.1+build-feature'), true);
  assert.equal(isPrerelease('1.2.3+build-feature'), false);
  assert.equal(readReleaseMetadata(writeReleaseFixture({ packageVersion: '1.2.3+build-feature' })).prerelease, false);
});

test('rejects a version mismatch across release metadata files', () => {
  const root = writeReleaseFixture({ cargoVersion: '1.2.4' });
  assert.throws(() => readReleaseMetadata(root), /Release versions must match.*Cargo\.toml \[workspace\.package\]=1\.2\.4/);
});

test('rejects non-strict SemVer, including a leading v and numeric leading zeroes', () => {
  for (const version of ['v1.2.3', '01.2.3', '1.2', '1.2.3-01']) {
    const root = writeReleaseFixture({ packageVersion: version });
    assert.throws(() => readReleaseMetadata(root), /not strict SemVer/);
  }
});

test('reads version specifically from the Cargo workspace.package section', () => {
  const cargo = `[package]\nversion = "9.9.9"\n\n[workspace]\nversion = "8.8.8"\n\n[workspace.package]\nversion = "2.3.4-beta.2"\n\n[profile.release]\nlto = true\n`;
  assert.equal(readCargoWorkspaceVersion(cargo), '2.3.4-beta.2');
});

test('requires exactly one Cargo workspace.package version', () => {
  assert.throws(
    () => readCargoWorkspaceVersion('[workspace.package]\nname = "AgentHub"\n'),
    /exactly one string version/,
  );
});
