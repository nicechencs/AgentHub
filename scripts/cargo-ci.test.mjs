import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = path.join(root, 'scripts', 'cargo-ci.sh');
const hasBash = spawnSync('bash', ['--version'], { stdio: 'ignore' }).status === 0;

function bashRelativePath(file) {
  const relative = path.relative(root, path.resolve(file));
  if (
    !relative
    || path.isAbsolute(relative)
    || relative === '..'
    || relative.startsWith(`..${path.sep}`)
  ) {
    throw new Error(`expected a path inside the repository root: ${file}`);
  }
  return relative.split(path.sep).join('/');
}

function run(args, extra = {}) {
  return spawnSync('bash', [bashRelativePath(script), ...args], {
    encoding: 'utf8',
    cwd: root,
    env: { ...process.env, ...(extra.env ?? {}) },
  });
}

function writeHelper(dir, name, body) {
  const file = path.join(dir, name);
  fs.writeFileSync(file, `#!/usr/bin/env bash\nset -euo pipefail\n${body}\n`);
  fs.chmodSync(file, 0o755);
  return file;
}

test('cargo-ci.sh exists and documents ETXTBSY-only retry', () => {
  const text = fs.readFileSync(script, 'utf8');
  assert.match(text, /^#!/);
  assert.match(text, /ETXTBSY/);
  assert.match(text, /Text file busy/);
  assert.match(text, /\bretry\b/);
  assert.match(text, /\bparallel\b/);
  assert.doesNotMatch(text, /cargo nextest/);
});

test('bash -n cargo-ci.sh', { skip: !hasBash }, () => {
  const result = spawnSync('bash', ['-n', bashRelativePath(script)], {
    encoding: 'utf8',
    cwd: root,
  });
  assert.equal(result.status, 0, result.stderr);
});

test('retry succeeds without retrying a clean command', { skip: !hasBash }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-ci-test-'));
  try {
    const counter = path.join(dir, 'counter');
    const helper = writeHelper(dir, 'ok.sh', `
echo 0 > '${counter}'
c=$(cat '${counter}')
echo $((c + 1)) > '${counter}'
echo ok
`);
    const result = run(['retry', '--attempts', '3', '--delay-seconds', '0', '--', 'bash', helper]);
    assert.equal(result.status, 0, result.stderr + result.stdout);
    assert.equal(fs.readFileSync(counter, 'utf8').trim(), '1');
    assert.doesNotMatch(result.stderr + result.stdout, /retry in /);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('retry repeats only on Text file busy, then succeeds', { skip: !hasBash }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-ci-test-'));
  try {
    const counter = path.join(dir, 'counter');
    fs.writeFileSync(counter, '0');
    const helper = writeHelper(dir, 'busy.sh', `
c=$(cat '${counter}')
c=$((c + 1))
echo "$c" > '${counter}'
if [ "$c" -lt 3 ]; then
  echo "error: failed to remove file target/debug/foo: Text file busy (os error 26)" >&2
  exit 1
fi
echo recovered
`);
    const result = run(['retry', '--attempts', '3', '--delay-seconds', '0', '--', 'bash', helper]);
    assert.equal(result.status, 0, result.stderr + result.stdout);
    assert.equal(fs.readFileSync(counter, 'utf8').trim(), '3');
    assert.match(result.stderr, /ETXTBSY\/Text file busy \(attempt 1\/3\)/);
    assert.match(result.stdout, /recovered/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('retry gives up after ETXTBSY attempts', { skip: !hasBash }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-ci-test-'));
  try {
    const counter = path.join(dir, 'counter');
    fs.writeFileSync(counter, '0');
    const helper = writeHelper(dir, 'always-busy.sh', `
c=$(cat '${counter}')
c=$((c + 1))
echo "$c" > '${counter}'
echo ETXTBSY >&2
exit 17
`);
    const result = run(['retry', '--attempts', '3', '--delay-seconds', '0', '--', 'bash', helper]);
    assert.equal(result.status, 17, result.stderr + result.stdout);
    assert.equal(fs.readFileSync(counter, 'utf8').trim(), '3');
    assert.match(result.stderr, /still ETXTBSY after 3 attempts/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('non-ETXTBSY failure is not retried', { skip: !hasBash }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-ci-test-'));
  try {
    const counter = path.join(dir, 'counter');
    fs.writeFileSync(counter, '0');
    const helper = writeHelper(dir, 'assert.sh', `
c=$(cat '${counter}')
c=$((c + 1))
echo "$c" > '${counter}'
echo "thread 'foo' panicked at tests.rs: assertion failed: queue is busy" >&2
exit 1
`);
    const result = run(['retry', '--attempts', '3', '--delay-seconds', '0', '--', 'bash', helper]);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.equal(fs.readFileSync(counter, 'utf8').trim(), '1');
    assert.doesNotMatch(result.stderr + result.stdout, /retry in /);
    assert.doesNotMatch(result.stderr + result.stdout, /still ETXTBSY/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('parallel fails the step if any command failed', { skip: !hasBash }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-ci-test-'));
  try {
    const ok = writeHelper(dir, 'ok.sh', 'echo ok-a; exit 0');
    const bad = writeHelper(dir, 'bad.sh', 'echo boom; exit 9');
    const result = run([
      'parallel',
      '--attempts',
      '2',
      '--delay-seconds',
      '0',
      '--',
      `bash ${ok}`,
      `bash ${bad}`,
    ]);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /cargo-ci: summary \[job-1\] exit 0/);
    assert.match(result.stdout, /cargo-ci: summary \[job-2\] exit 9/);
    assert.match(result.stderr, /one or more parallel commands failed/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('parallel succeeds when every command succeeds', { skip: !hasBash }, () => {
  const result = run([
    'parallel',
    '--delay-seconds',
    '0',
    '--',
    'echo core; true',
    'echo gui; true',
  ]);
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.match(result.stdout, /cargo-ci: summary \[job-1\] exit 0/);
  assert.match(result.stdout, /cargo-ci: summary \[job-2\] exit 0/);
});

test('parallel labels cargo -p package names', { skip: !hasBash }, () => {
  const result = run([
    'parallel',
    '--delay-seconds',
    '0',
    '--',
    'echo cargo test -p agenthub-core --locked',
  ]);
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.match(result.stdout, /\[agenthub-core\]/);
});

test('unknown subcommand exits 2', { skip: !hasBash }, () => {
  const result = run(['nope']);
  assert.equal(result.status, 2);
  assert.match(result.stderr, /unknown command/);
});
