#!/usr/bin/env node

// A deterministic stdio JSON-RPC peer used only by chat-codex-probe.test.mjs.
// It intentionally implements the small schema-confirmed read-only surface
// that the production probe is allowed to call.
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const mode = process.argv[2] || 'success';
let buffer = '';

function writeMessage(message) {
  const text = `${JSON.stringify(message)}\n`;
  if (mode === 'fragmented') {
    const split = Math.max(1, Math.floor(text.length / 2));
    process.stdout.write(text.slice(0, split));
    setTimeout(() => process.stdout.write(text.slice(split)), 1);
  } else {
    process.stdout.write(text);
  }
}

function threadResult(cwd) {
  return {
    approvalPolicy: 'untrusted',
    approvalsReviewer: 'user',
    cwd,
    model: null,
    modelProvider: 'openai',
    sandbox: 'read-only',
    thread: {
      cliVersion: '0.153.0',
      createdAt: 0,
      cwd,
      ephemeral: true,
      id: 'fake-thread-id',
      modelProvider: 'openai',
      preview: '',
      projectId: null,
      sessionId: 'fake-session-id',
      source: 'app-server',
      status: 'idle',
      turns: [],
      updatedAt: 0,
    },
  };
}

function handle(message) {
  if (!message || typeof message !== 'object') return;
  if (message.method === 'initialized') return;
  if (typeof message.id !== 'number' || typeof message.method !== 'string') return;

  if (mode === 'timeout' && message.method === 'model/list') return;
  if (mode === 'half-line' && message.method === 'initialize') {
    process.stdout.write('{"id":1');
    return;
  }
  if (mode === 'exit') process.exit(17);
  if (mode === 'error' && message.method === 'model/list') {
    writeMessage({ id: message.id, error: { code: -32001, message: 'fake upstream failure' } });
    return;
  }

  let result;
  switch (message.method) {
    case 'initialize':
      if (mode === 'tree') {
        const descendant = spawn(process.execPath, ['-e', 'setInterval(() => {}, 60_000)'], {
          stdio: 'ignore',
        });
        if (process.argv[3]) writeFileSync(process.argv[3], String(descendant.pid));
      }
      result = {
        codexHome: process.env.CODEX_HOME,
        platformFamily: process.platform === 'win32' ? 'windows' : 'unix',
        platformOs: process.platform,
        userAgent: 'codex-cli/0.153.0',
      };
      break;
    case 'model/list':
      result = { data: [], nextCursor: null };
      break;
    case 'skills/list':
      result = { data: [] };
      break;
    case 'plugin/installed':
      result = { marketplaces: [] };
      break;
    case 'thread/start':
      result = threadResult(message.params?.cwd);
      break;
    default:
      writeMessage({ id: message.id, error: { code: -32601, message: 'unknown method' } });
      return;
  }
  writeMessage({ id: message.id, result });
}

if (mode === 'stderr') process.stderr.write('x'.repeat(1024));

process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  buffer += chunk;
  let newline = buffer.indexOf('\n');
  while (newline !== -1) {
    const line = buffer.slice(0, newline);
    buffer = buffer.slice(newline + 1);
    if (line.trim()) {
      try {
        handle(JSON.parse(line));
      } catch {
        process.exit(19);
      }
    }
    newline = buffer.indexOf('\n');
  }
});
process.stdin.resume();
