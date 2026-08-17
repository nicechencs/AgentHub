import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = path.join(root, 'scripts', 'check-linux-prereqs.sh');
const launcher = path.join(root, 'run.sh');

function run(file, args, extra = {}) {
  return spawnSync('bash', [file, ...args], {
    encoding: 'utf8',
    cwd: root,
    env: { ...process.env, ...(extra.env ?? {}) },
  });
}

test('linux prereq script exists and is a bash checker', () => {
  const text = fs.readFileSync(script, 'utf8');
  assert.match(text, /^#!/);
  assert.match(text, /webkit2gtk-4\.1/);
  assert.match(text, /libayatana-appindicator3-dev/);
  assert.match(text, /sudo apt-get install/);
  assert.match(text, /sudo dnf install/);
  assert.match(text, /sudo pacman -S/);
  assert.match(text, /zypper|apk/);
  assert.match(text, /do not assume apt-get/i);
  assert.doesNotMatch(text, /\$\(\s*sudo|`sudo/);
  assert.match(text, /never uses sudo/i);
});

test('--print-packages lists Debian, Fedora, and Arch commands', () => {
  const result = run(script, ['--print-packages']);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /apt-get install/);
  assert.match(result.stdout, /libwebkit2gtk-4\.1-dev/);
  assert.match(result.stdout, /dnf install/);
  assert.match(result.stdout, /pacman -S/);
  assert.match(result.stdout, /zypper|apk/);
  assert.match(result.stdout, /do not assume apt-get/i);
  assert.equal(result.stdout.includes('TAURI_SIGNING'), false);
});

test('--check fails closed on non-Linux hosts', () => {
  if (os.platform() === 'linux') {
    const result = run(script, ['--check']);
    assert.notEqual(result.status, 2);
    if (result.status !== 0) {
      assert.match(`${result.stdout}\n${result.stderr}`, /missing |incomplete|Install the packages/i);
      assert.match(`${result.stdout}\n${result.stderr}`, /apt-get|dnf|pacman/);
    } else {
      assert.match(`${result.stdout}\n${result.stderr}`, /look ready/);
    }
    return;
  }
  const result = run(script, ['--check']);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /for Linux/);
});

test('unknown flags fail with usage, not a package-manager mutation', () => {
  const result = run(script, ['--please-install']);
  assert.equal(result.status, 2);
  assert.match(result.stderr, /Unknown argument/);
  assert.doesNotMatch(`${result.stdout}\n${result.stderr}`, /^\s*sudo\s/m);
});

test('run.sh --help documents Linux source-run, Releases, and --check', () => {
  const result = run(launcher, ['--help']);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /--check/);
  assert.match(result.stdout, /Linux/);
  assert.match(result.stdout, /check-linux-prereqs/);
  assert.match(result.stdout, /GitHub Releases/);
  assert.match(result.stdout, /AppImage/);
  assert.doesNotMatch(result.stdout, /Windows\/macOS only/);
});
