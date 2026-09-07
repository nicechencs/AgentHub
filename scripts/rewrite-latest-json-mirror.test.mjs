import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import {
  filenameFromUrl,
  normalizeMirrorBase,
  rewriteLatestJson,
} from './rewrite-latest-json-mirror.mjs';

const script = fileURLToPath(new URL('./rewrite-latest-json-mirror.mjs', import.meta.url));

test('normalizeMirrorBase strips trailing slash and path', () => {
  assert.equal(
    normalizeMirrorBase('https://updates.agenthub.qooo.io/'),
    'https://updates.agenthub.qooo.io',
  );
  assert.equal(
    normalizeMirrorBase('https://updates.agenthub.qooo.io/updates/'),
    'https://updates.agenthub.qooo.io/updates',
  );
});

test('filenameFromUrl keeps release asset basename', () => {
  assert.equal(
    filenameFromUrl(
      'https://github.com/nicechencs/AgentHub/releases/download/v0.4.8/AgentHub_0.4.8_x64-setup.exe',
    ),
    'AgentHub_0.4.8_x64-setup.exe',
  );
});

test('rewriteLatestJson rewrites only platform urls', () => {
  const feed = {
    version: '0.4.8',
    notes: 'AgentHub v0.4.8',
    pub_date: '2026-09-07T00:00:00.000Z',
    platforms: {
      'windows-x86_64': {
        signature: 'win-sig',
        url: 'https://github.com/nicechencs/AgentHub/releases/download/v0.4.8/AgentHub_0.4.8_x64-setup.exe',
      },
      'darwin-aarch64': {
        signature: 'mac-sig',
        url: 'https://github.com/nicechencs/AgentHub/releases/download/v0.4.8/AgentHub_0.4.8_aarch64.app.tar.gz',
      },
    },
  };
  const rewritten = rewriteLatestJson(feed, 'https://updates.agenthub.qooo.io/');
  assert.equal(rewritten.version, '0.4.8');
  assert.equal(rewritten.platforms['windows-x86_64'].signature, 'win-sig');
  assert.equal(
    rewritten.platforms['windows-x86_64'].url,
    'https://updates.agenthub.qooo.io/AgentHub_0.4.8_x64-setup.exe',
  );
  assert.equal(
    rewritten.platforms['darwin-aarch64'].url,
    'https://updates.agenthub.qooo.io/AgentHub_0.4.8_aarch64.app.tar.gz',
  );
  // Original feed object is not mutated.
  assert.match(feed.platforms['windows-x86_64'].url, /github\.com/);
});

test('CLI rewrites file in place', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'agenthub-mirror-json-'));
  const file = path.join(dir, 'latest.json');
  fs.writeFileSync(
    file,
    JSON.stringify({
      version: '1.0.0',
      notes: '',
      pub_date: '2026-01-01T00:00:00.000Z',
      platforms: {
        'windows-x86_64': {
          signature: 's',
          url: 'https://example.invalid/r/AgentHub_1.0.0_x64-setup.exe',
        },
      },
    }),
    'utf8',
  );
  const result = spawnSync(
    process.execPath,
    [
      script,
      '--in',
      file,
      '--out',
      file,
      '--mirror-base-url',
      'https://updates.agenthub.qooo.io',
    ],
    { encoding: 'utf8' },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const out = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert.equal(
    out.platforms['windows-x86_64'].url,
    'https://updates.agenthub.qooo.io/AgentHub_1.0.0_x64-setup.exe',
  );
});
