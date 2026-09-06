import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import {
  VERIFIED_PROTOCOL_ONLY,
  runCodexProbe,
} from './chat-codex-probe.mjs';

const fixturePath = fileURLToPath(new URL('./fixtures/chat-codex-probe/fake-app-server.mjs', import.meta.url));
const probePath = fileURLToPath(new URL('./chat-codex-probe.mjs', import.meta.url));

async function runFixture(mode, options = {}) {
  const { fixtureArgs = [], ...probeOptions } = options;
  return runCodexProbe({
    executable: process.execPath,
    executableArgs: [fixturePath, mode, ...fixtureArgs],
    requestTimeoutMs: 500,
    ...probeOptions,
  });
}

function noCloseSpawn() {
  const child = new EventEmitter();
  child.pid = 2_147_000_000;
  child.stdin = {
    destroyed: false,
    write(_payload, _encoding, callback) {
      callback();
    },
    end() {},
  };
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kill = () => false;
  return child;
}

test('CLI exposes a safe help message', () => {
  const result = spawnSync(process.execPath, [probePath, '--help'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /read-only JSON-RPC protocol/);
  assert.match(result.stdout, /--executable PATH/);
  assert.doesNotMatch(result.stdout, /CODEX_HOME|OPENAI_API_KEY/);
});

test('CLI keeps app-server argv fixed instead of accepting arbitrary --arg values', () => {
  const result = spawnSync(process.execPath, [probePath, '--arg', 'exec'], { encoding: 'utf8' });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /invalid arguments/);
});

test('performs the schema-confirmed handshake in order with fragmented JSON', async () => {
  const summary = await runFixture('fragmented');

  assert.equal(summary.status, 'ok');
  assert.equal(summary.cliVersion, '0.153.0');
  assert.equal(summary.methods.initialize.status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods.initialized.status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['model/list'].status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['skills/list'].status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['plugin/installed'].status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['thread/start'].status, VERIFIED_PROTOCOL_ONLY);
  assert.deepEqual(summary.wireOrder, [
    'initialize',
    'initialized',
    'model/list',
    'skills/list',
    'plugin/installed',
    'thread/start',
  ]);
  assert.equal(summary.counts.requestsSent, 5);
  assert.equal(summary.counts.notificationsSent, 1);
  assert.ok(summary.unverified.includes('model execution'));
  assert.ok(summary.unverified.includes('approvals'));
  assert.ok(summary.unverified.includes('session recovery'));
  assert.equal(summary.cleanup, 'ok');
});

test('reports an upstream JSON-RPC error without leaking its message', async () => {
  const summary = await runFixture('error');

  assert.equal(summary.status, 'failed');
  assert.equal(summary.methods['model/list'].status, 'failed');
  assert.equal(summary.methods['model/list'].reason, 'upstream_error');
  assert.equal(summary.methods['model/list'].errorCode, -32001);
  assert.equal(summary.methods['model/list'].message, undefined);
  assert.equal(summary.methods['skills/list'].status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['thread/start'].status, VERIFIED_PROTOCOL_ONLY);
});

test('times out an incomplete request and removes the isolated tree', async () => {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'agenthub-chat-codex-probe-test-'));
  const summary = await runFixture('timeout', { tempRoot, requestTimeoutMs: 80 });

  assert.equal(summary.status, 'failed');
  assert.equal(summary.methods.initialize.status, VERIFIED_PROTOCOL_ONLY);
  assert.equal(summary.methods['model/list'].reason, 'timeout');
  assert.equal(summary.cleanup, 'ok');
  assert.deepEqual(await fs.readdir(tempRoot), []);
  await fs.rm(tempRoot, { recursive: true, force: true });
});

test('handles a half-line JSON message conservatively', async () => {
  const summary = await runFixture('half-line');

  assert.equal(summary.status, 'failed');
  assert.equal(summary.methods.initialize.status, 'failed');
  assert.equal(summary.methods.initialize.reason, 'timeout');
  assert.equal(summary.counts.invalidMessages, 0);
});

test('reports an abnormal app-server exit without treating it as success', async () => {
  const summary = await runFixture('exit');

  assert.equal(summary.status, 'failed');
  assert.equal(summary.methods.initialize.status, 'failed');
  assert.equal(summary.methods.initialize.reason, 'unexpected_exit');
  assert.equal(summary.cleanup, 'ok');
});

test('classifies an executable startup failure without leaking an error message', async () => {
  const summary = await runCodexProbe({
    executable: path.join(os.tmpdir(), 'agenthub-chat-codex-probe-does-not-exist'),
    requestTimeoutMs: 200,
  });

  assert.equal(summary.status, 'failed');
  assert.equal(summary.methods.initialize.reason, 'spawn_error');
  assert.equal(summary.cleanup, 'ok');
});

test('kills a descendant in the app-server process group before cleanup', async () => {
  const pidFile = path.join(os.tmpdir(), `agenthub-chat-codex-probe-${process.pid}.pid`);
  await fs.rm(pidFile, { force: true });
  const summary = await runFixture('tree', { fixtureArgs: [pidFile] });

  assert.equal(summary.status, 'ok');
  assert.equal(summary.cleanup, 'ok');
  const pid = Number(await fs.readFile(pidFile, 'utf8'));
  assert.ok(Number.isInteger(pid) && pid > 0);
  await fs.rm(pidFile, { force: true });
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
});

test('reports cleanup failure and preserves the tree when close never arrives', async () => {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'agenthub-chat-codex-probe-no-close-'));
  try {
    const summary = await runCodexProbe({
      executable: 'fake-no-close',
      tempRoot,
      requestTimeoutMs: 20,
      spawnImpl: noCloseSpawn,
    });

    assert.equal(summary.status, 'failed');
    assert.equal(summary.methods.initialize.reason, 'timeout');
    assert.equal(summary.cleanup, 'failed');
    assert.equal((await fs.readdir(tempRoot)).length, 1);
  } finally {
    await fs.rm(tempRoot, { recursive: true, force: true });
  }
});

test('limits stderr and still cleans up the child process', async () => {
  const summary = await runFixture('stderr', { stderrLimitBytes: 16 });

  assert.equal(summary.status, 'failed');
  assert.equal(summary.fatal, undefined);
  assert.ok(summary.counts.stderrBytes > 16);
  assert.equal(summary.methods.initialize.status, 'failed');
  assert.equal(summary.methods.initialize.reason, 'stderr_limit');
  assert.equal(summary.cleanup, 'ok');
});
