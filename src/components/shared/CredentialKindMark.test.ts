import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { credentialKindFromClass } from './CredentialKindMark';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('credentialKindFromClass', () => {
  it('maps ticket classes onto connection kinds', () => {
    expect(credentialKindFromClass('oauth')).toBe('oauth');
    expect(credentialKindFromClass('api_key')).toBe('apikey');
    expect(credentialKindFromClass('apikey')).toBe('apikey');
    expect(credentialKindFromClass('unknown')).toBeNull();
    expect(credentialKindFromClass(null)).toBeNull();
  });
});

describe('CredentialKindMark wiring', () => {
  it('is the shared official-login / API Key mark on Connections and the pool', () => {
    expect(source('pages/connections/TicketWalletList.tsx')).toContain('<CredentialKindMark');
    expect(source('pages/connections/TicketWalletList.tsx')).not.toContain('function CredentialMark');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).toContain('<CredentialKindMark');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).not.toContain('function KindMark');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).toContain('data-pool-kind-mark');
  });
});
