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
        label: 'Grok · OAuth',
        extra: { identityLabel: 'a@example.com', email: 'a@example.com' },
      }),
    );
    expect(withLabel.identityLabel).toBe('a@example.com');
    expect(withLabel.email).toBe('a@example.com');
    // List title prefers account email when present.
    expect(withLabel.label).toBe('a@example.com');
    expect(withLabel.createdAt).toBe('2026-08-02 09:00:00.000000');

    const fallback = mapCoreAccount(core({ id: 'a2', label: 'only-label', extra: {} }));
    expect(fallback.identityLabel).toBe('only-label');
    expect(fallback.label).toBe('only-label');
  });

  it('reads email / plan from credentials when extra omits them', () => {
    const mapped = mapCoreAccount(
      core({
        id: 'codex-1',
        agentId: 'codex',
        label: 'Codex · OAuth',
        credentials: {
          type: 'oauth',
          email: 'c@openai.test',
          plan_type: 'plus',
        },
        extra: { source: 'oauth_pkce' },
      }),
    );
    expect(mapped.email).toBe('c@openai.test');
    expect(mapped.label).toBe('c@openai.test');
    expect(mapped.subscription).toBe('plus');
    expect(mapped.identityLabel).toBe('c@openai.test');
  });

  it('surfaces Pi provider + subject from auth_json body keys', () => {
    const mapped = mapCoreAccount(
      core({
        id: 'pi-1',
        agentId: 'pi',
        label: 'pi:xai (oauth)',
        credentials: {
          format: 'auth_json',
          body: { xai: { type: 'oauth', access: '***', refresh: '***' } },
        },
        extra: {
          source: 'live',
          identityLabel: '36b45542…',
          provider: 'xai',
          sub: '36b45542-a4c3-4a5d-b4d9-1c685d10dcd9',
          subscription: 'tier 5',
        },
      }),
    );
    expect(mapped.provider).toBe('xai');
    expect(mapped.subscription).toBe('tier 5');
    expect(mapped.subjectId).toContain('36b45542');
    // Weak legacy label upgraded using provider + identity.
    expect(mapped.label).toContain('xai');
    expect(mapped.identityLabel).toBe('36b45542…');
  });

  it('prefers email over placeholder label and UUID identityLabel', () => {
    const mapped = mapCoreAccount(
      core({
        id: 'codex-1',
        agentId: 'codex',
        label: 'codex-oauth',
        credentials: { format: 'auth_json', body: { tokens: {} } },
        extra: {
          email: '41375197@qq.com',
          identityLabel: 'fcf2a4f8-bbff-4598-910d-067e947e229c',
          subscription: 'prolite',
        },
      }),
    );
    expect(mapped.label).toBe('41375197@qq.com');
    expect(mapped.email).toBe('41375197@qq.com');
    expect(mapped.identityLabel).toBe('41375197@qq.com');
    expect(mapped.subscription).toBe('prolite');
  });

  it('derives tokenRemainingSec from expiresAt (RFC3339)', () => {
    const exp = new Date(Date.now() + 2 * 3600 * 1000 + 5 * 60 * 1000).toISOString();
    const mapped = mapCoreAccount(
      core({
        id: 'codex-exp',
        agentId: 'codex',
        label: 'c@x.com',
        extra: { expiresAt: exp, email: 'c@x.com' },
      }),
    );
    expect(mapped.tokenRemainingSec).toBeDefined();
    // ~2h05m → between 2h and 2h10m
    expect(mapped.tokenRemainingSec!).toBeGreaterThan(2 * 3600);
    expect(mapped.tokenRemainingSec!).toBeLessThan(2 * 3600 + 10 * 60);
    expect(mapped.tokenValid).toBe(true);
  });

  it('recomputes quotaResetIn from absolute quota5hResetAt', () => {
    const resetAt = new Date(Date.now() + 90 * 60 * 1000).toISOString();
    const mapped = mapCoreAccount(
      core({
        id: 'codex-q',
        agentId: 'codex',
        label: 'c@x.com',
        extra: {
          quota5hPct: 40,
          quota5hResetAt: resetAt,
          // Stale frozen label from probe time — must be recomputed.
          quotaResetIn: '9h00m 后重置',
        },
      }),
    );
    expect(mapped.quotaResetIn).toMatch(/^1h/);
    expect(mapped.quotaResetIn).toContain('后重置');
    expect(mapped.quota5hPct).toBe(40);
  });

  it('does not use 7d reset for the 5h quotaResetIn bar', () => {
    const reset7d = new Date(Date.now() + 9 * 24 * 3600 * 1000).toISOString();
    const mapped = mapCoreAccount(
      core({
        id: 'codex-7d',
        agentId: 'codex',
        label: 'c@x.com',
        extra: {
          quota7dPct: 30,
          quota7dResetAt: reset7d,
          // No 5h reset — must not fall back to weekly "9d".
          quotaResetIn: undefined,
        },
      }),
    );
    expect(mapped.quotaResetIn).toBeUndefined();
    expect(mapped.quota7dPct).toBe(30);
  });

  it('upgrades grok-oauth title when email is in extra', () => {
    const mapped = mapCoreAccount(
      core({
        id: 'grok-1',
        agentId: 'grok',
        label: 'grok-oauth',
        extra: {
          email: 'cunsen.chen@gmail.com',
          identityLabel: 'cunsen.chen@gmail.com',
        },
      }),
    );
    expect(mapped.label).toBe('cunsen.chen@gmail.com');
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
