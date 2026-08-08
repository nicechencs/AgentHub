import { describe, expect, it } from 'vitest';
import { groupAccountsByIdentity, mapCoreAccount, type CoreAccount } from './account-map';

function core(partial: Partial<CoreAccount> & Pick<CoreAccount, 'id'>): CoreAccount {
  return {
    agentId: 'grok',
    kind: 'oauth',
    label: 'grok-oauth',
    status: 'active',
    isCurrent: false,
    createdAt: '2026-08-02 09:00:00.000000',
    updatedAt: '2026-08-02 09:00:00.000000',
    ...partial,
  };
}

describe('mapCoreAccount', () => {
  it('maps identityLabel from extra and falls back to label', () => {
    const withLabel = mapCoreAccount(
      core({
        id: 'a1',
        extra: { identityLabel: 'a@example.com', email: 'a@example.com' },
      }),
    );
    expect(withLabel.identityLabel).toBe('a@example.com');
    expect(withLabel.email).toBe('a@example.com');
    expect(withLabel.createdAt).toBe('2026-08-02 09:00:00.000000');

    const fallback = mapCoreAccount(core({ id: 'a2', label: 'only-label', extra: {} }));
    expect(fallback.identityLabel).toBe('only-label');
  });

  it('maps credential format / source / env_key for detail panel', () => {
    const mapped = mapCoreAccount(
      core({
        id: 'claude-1',
        agentId: 'claude',
        kind: 'apikey',
        label: 'sk-••••',
        credentials: {
          format: 'api_key',
          api_key: '***',
          env_key: 'ANTHROPIC_AUTH_TOKEN',
        },
        extra: { source: 'settings.json', identityLabel: 'sk-••••' },
      }),
    );
    expect(mapped.credentialFormat).toBe('api_key');
    expect(mapped.envKey).toBe('ANTHROPIC_AUTH_TOKEN');
    expect(mapped.source).toBe('settings.json');
    expect(mapped.credentialSummary).toContain('format=api_key');
    expect(mapped.credentialSummary).toContain('env=ANTHROPIC_AUTH_TOKEN');
    expect(mapped.status).toBe('active');
  });

  it('derives tokenRemainingSec from expiresAt and marks tokenExpired', () => {
    const futureSec = Math.floor(Date.now() / 1000) + 3600;
    const ok = mapCoreAccount(
      core({
        id: 'o1',
        credentials: { format: 'credentials_json', body: { claudeAiOauth: { accessToken: '***' } } },
        extra: { source: '.credentials.json', expiresAt: futureSec },
      }),
    );
    expect(ok.tokenValid).toBe(true);
    expect(ok.tokenRemainingSec).toBeGreaterThan(3000);
    expect(ok.credentialFormat).toBe('credentials_json');
    expect(ok.credentialSummary).toContain('bodyKeys=claudeAiOauth');

    const expired = mapCoreAccount(
      core({
        id: 'o2',
        extra: { tokenExpired: true, expiresAt: 1 },
      }),
    );
    expect(expired.tokenValid).toBe(false);
    expect(expired.tokenRemainingSec).toBeLessThan(0);
  });
});

describe('groupAccountsByIdentity', () => {
  it('groups same identity and sorts current first', () => {
    const accounts = [
      mapCoreAccount(
        core({
          id: 'old',
          isCurrent: false,
          updatedAt: '2026-08-02 09:00:00.000000',
          extra: { identityLabel: 'a@example.com' },
        }),
      ),
      mapCoreAccount(
        core({
          id: 'new',
          isCurrent: true,
          updatedAt: '2026-08-02 10:00:00.000000',
          extra: { identityLabel: 'a@example.com' },
        }),
      ),
      mapCoreAccount(
        core({
          id: 'other',
          label: 'b@example.com',
          extra: { identityLabel: 'b@example.com' },
        }),
      ),
    ];
    const groups = groupAccountsByIdentity(accounts);
    expect(groups).toHaveLength(2);
    expect(groups[0]!.identity).toBe('a@example.com');
    expect(groups[0]!.accounts.map((a) => a.id)).toEqual(['new', 'old']);
    expect(groups[1]!.identity).toBe('b@example.com');
  });

  it('falls back to label when identityLabel missing and keeps multi-auth under same label', () => {
    const accounts = [
      mapCoreAccount(
        core({
          id: 't1',
          label: 'same-label',
          isCurrent: false,
          updatedAt: '2026-08-01 00:00:00.000000',
          extra: {},
        }),
      ),
      mapCoreAccount(
        core({
          id: 't2',
          label: 'same-label',
          isCurrent: true,
          updatedAt: '2026-08-02 00:00:00.000000',
          extra: {},
        }),
      ),
    ];
    const groups = groupAccountsByIdentity(accounts);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.identity).toBe('same-label');
    expect(groups[0]!.accounts).toHaveLength(2);
    expect(groups[0]!.accounts[0]!.id).toBe('t2');
  });
});
