import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { canSyncConnectionToPool } from '@/components/login-kernel';

const repoSrc = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function filesUnder(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules') continue;
      out.push(...filesUnder(full));
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry.name) && !entry.name.includes('.test.')) out.push(full);
  }
  return out;
}

describe('pool shareable login rule', () => {
  it('keeps the sync rule only in the login kernel', () => {
    const hits = filesUnder(repoSrc).filter((file) => {
      const src = readFileSync(file, 'utf8');
      return src.includes('POOL_SHAREABLE_OAUTH_AGENTS');
    });
    expect(hits.map((file) => path.relative(repoSrc, file))).toEqual([
      path.join('components', 'login-kernel', 'eligibility.ts'),
    ]);
    expect(canSyncConnectionToPool({ agentId: 'workbuddy', kind: 'apikey' })).toBe(true);
    expect(canSyncConnectionToPool({ agentId: 'kimi', kind: 'oauth' })).toBe(false);
    expect(canSyncConnectionToPool({ agentId: 'claude', kind: 'oauth', home: 'route_pool' })).toBe(false);
  });
});
