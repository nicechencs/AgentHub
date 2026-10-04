import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  parseArgs,
  parseRustHost,
  sha256File,
  sidecarFileName,
  sidecarPaths,
  targetSpec,
  verifySidecar,
} from './build-go-sidecar.mjs';

test('maps every release target to the native Go target and Tauri filename', () => {
  assert.deepEqual(targetSpec('x86_64-unknown-linux-gnu'), {
    goos: 'linux', goarch: 'amd64', extension: '',
  });
  assert.deepEqual(targetSpec('aarch64-apple-darwin'), {
    goos: 'darwin', goarch: 'arm64', extension: '',
  });
  assert.deepEqual(targetSpec('x86_64-pc-windows-msvc'), {
    goos: 'windows', goarch: 'amd64', extension: '.exe',
  });
  assert.equal(sidecarFileName('x86_64-pc-windows-msvc'), 'agenthub-adapterd-x86_64-pc-windows-msvc.exe');
  assert.throws(() => targetSpec('wasm32-unknown-unknown'), /Unsupported desktop sidecar target/);
});

test('parses the rust host without guessing from the current Node architecture', () => {
  assert.equal(parseRustHost('rustc 1.80.0\nbinary: rustc\nhost: aarch64-apple-darwin\n'), 'aarch64-apple-darwin');
  assert.throws(() => parseRustHost('rustc 1.80.0\n'), /did not report a host target/);
});

test('verifies the manifest version, target, size, and digest', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-sidecar-test-'));
  fs.mkdirSync(path.join(root, 'src-tauri', 'binaries'), { recursive: true });
  fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ version: '1.2.3' }));
  const target = 'x86_64-unknown-linux-gnu';
  const paths = sidecarPaths(root, target);
  fs.writeFileSync(paths.binary, 'sidecar fixture 1.2.3');
  const manifest = {
    schemaVersion: 1,
    target,
    goos: 'linux',
    goarch: 'amd64',
    packageVersion: '1.2.3',
    fileName: path.basename(paths.binary),
    sha256: sha256File(paths.binary),
    size: fs.statSync(paths.binary).size,
  };
  fs.writeFileSync(paths.manifest, JSON.stringify(manifest));
  assert.equal(verifySidecar(root, target).manifest.sha256, manifest.sha256);
  fs.appendFileSync(paths.binary, 'tampered');
  assert.throws(() => verifySidecar(root, target), /sha256 mismatch/);

  fs.writeFileSync(paths.binary, 'sidecar without injected version');
  manifest.sha256 = sha256File(paths.binary);
  manifest.size = fs.statSync(paths.binary).size;
  fs.writeFileSync(paths.manifest, JSON.stringify(manifest));
  assert.throws(() => verifySidecar(root, target), /does not contain the injected desktop version/);
});

test('parses check, root, and target arguments and rejects unknown flags', () => {
  const parsed = parseArgs(['--check', '--root', '.', '--target', 'x86_64-unknown-linux-gnu']);
  assert.equal(parsed.check, true);
  assert.equal(parsed.target, 'x86_64-unknown-linux-gnu');
  assert.equal(parsed.root, path.resolve('.'));
  assert.throws(() => parseArgs(['--wat']), /Unknown argument/);
  assert.throws(() => parseArgs(['--target']), /requires a value/);
});
