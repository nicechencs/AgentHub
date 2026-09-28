import { describe, expect, it } from 'vitest';
import { canAddApiKey, canStartOfficialLogin, canSyncConnectionToPool } from './eligibility';

describe('canSyncConnectionToPool', () => {
  it('allows an API Key from any Agent', () => {
    for (const agentId of ['workbuddy', 'zcode', 'pi', 'kimi', 'cursor', 'claude'] as const) {
      expect(canSyncConnectionToPool({ agentId, kind: 'apikey' })).toBe(true);
    }
  });

  it('allows only Claude, Codex, and Grok official logins', () => {
    expect(canSyncConnectionToPool({ agentId: 'claude', kind: 'oauth' })).toBe(true);
    expect(canSyncConnectionToPool({ agentId: 'codex', kind: 'oauth' })).toBe(true);
    expect(canSyncConnectionToPool({ agentId: 'grok', kind: 'oauth' })).toBe(true);
    for (const agentId of ['kimi', 'workbuddy', 'zcode', 'pi', 'cursor'] as const) {
      expect(canSyncConnectionToPool({ agentId, kind: 'oauth' })).toBe(false);
    }
  });

  it('keeps a route-pool home out of the sync candidates', () => {
    expect(canSyncConnectionToPool({
      agentId: 'claude',
      kind: 'oauth',
      home: 'route_pool',
    })).toBe(false);
    expect(canSyncConnectionToPool({
      agentId: 'workbuddy',
      kind: 'apikey',
      home: 'route_pool',
    })).toBe(false);
  });
});

describe('add-menu eligibility', () => {
  it('keeps Cursor off the API Key form and follows the official-login support list', () => {
    expect(canAddApiKey('cursor')).toBe(false);
    expect(canAddApiKey('claude')).toBe(true);
    expect(canAddApiKey('kimi')).toBe(true);
    expect(canAddApiKey('workbuddy')).toBe(true);
    const supported = ['claude', 'codex', 'grok'] as const;
    expect(canStartOfficialLogin('claude', supported)).toBe(true);
    expect(canStartOfficialLogin('kimi', supported)).toBe(false);
    expect(canStartOfficialLogin('cursor', supported)).toBe(false);
    expect(canStartOfficialLogin('claude', [])).toBe(false);
  });
});
