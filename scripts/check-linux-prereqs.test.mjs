import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = path.join(root, 'scripts', 'check-linux-prereqs.sh');
const launcher = path.join(root, 'run.sh');
const fixtureRoot = fs.mkdtempSync(path.join(root, '.check-linux-prereqs-test-'));
const fixtureScript = path.join(fixtureRoot, 'scripts', 'check-linux-prereqs.sh');
const fixtureLauncher = path.join(fixtureRoot, 'run.sh');

function normalizeShell(text) {
  return text.replace(/\r\n?/g, '\n');
}

fs.mkdirSync(path.dirname(fixtureScript), { recursive: true });
fs.writeFileSync(fixtureScript, normalizeShell(fs.readFileSync(script, 'utf8')));
fs.writeFileSync(fixtureLauncher, normalizeShell(fs.readFileSync(launcher, 'utf8')));

after(() => {
  fs.rmSync(fixtureRoot, { recursive: true, force: true });
});

function bashRelativePath(file) {
  const relative = path.relative(root, path.resolve(file));
  if (
    !relative ||
    path.isAbsolute(relative) ||
    relative === '..' ||
    relative.startsWith(`..${path.sep}`)
  ) {
    throw new Error(`expected a path inside the repository root: ${file}`);
  }
  return relative.split(path.sep).join('/');
}

function run(file, args, extra = {}) {
  const resolved = path.resolve(file);
  const fixture = resolved === path.resolve(script)
    ? fixtureScript
    : resolved === path.resolve(launcher)
      ? fixtureLauncher
      : file;
  return spawnSync('bash', [bashRelativePath(fixture), ...args], {
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
  const output = `${result.stdout}\n${result.stderr}`;
  // Git Bash/WSL can report Linux even when Node is running on Windows, so
  // assert the exact branch reached by the checker rather than inferring it
  // from os.platform(). Both branches have a specific, actionable error.
  if (/Linux native build dependencies are incomplete\./.test(output)) {
    assert.match(output, /\[ERROR\] Linux native build dependencies are incomplete\./);
    assert.match(output, /apt-get|dnf|pacman/);
  } else {
    assert.match(output, /\[ERROR\] This checker is for Linux\. Detected [^\r\n]+\./);
  }
  assert.equal(fs.readFileSync(fixtureScript, 'utf8').includes('\r'), false);
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
