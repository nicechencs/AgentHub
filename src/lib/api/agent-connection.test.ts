import { describe, expect, it } from 'vitest';

import {
  applyEffectiveConnection,
  enrichStatusesWithConnections,
  extractProviderEndpoint,
  formatApiConnectionLabel,
  formatEndpointHost,
  formatLocalRouteLabel,
  isInternalGeneratedProvider,
  resolveEffectiveConnection,
} from '@/lib/api/agent-connection';
import type { Account, AgentStatus, Provider } from '@/lib/types';

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'me@example.com',
    isCurrent: true,
    tokenValid: true,
    updatedAt: '2026-08-01 10:00:00.000000',
    ...overrides,
  };
}

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: 'p-1',
    agentId: 'claude',
    name: 'xx云中转',
    preset: 'anthropic-compatible',
    configFormat: 'json',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com/v1',
        ANTHROPIC_AUTH_TOKEN: '***',
      },
    }),
    isCurrent: true,
    updatedAt: '2026-08-01 09:00:00.000000',
    ...overrides,
  };
}

function status(overrides: Partial<AgentStatus> = {}): AgentStatus {
  return {
    agentId: 'claude',
    installed: true,
    version: '2.0.0',
    authStatus: 'none',
    authLabel: '未检测登录态',
    running: false,
    envReady: true,
    ...overrides,
  };
}

describe('extractProviderEndpoint', () => {
  it('reads ANTHROPIC_BASE_URL from JSON env', () => {
    const text = JSON.stringify({
      env: { ANTHROPIC_BASE_URL: 'https://api.relay.test/v1' },
    });
    expect(extractProviderEndpoint(text, 'json')).toBe('https://api.relay.test/v1');
  });

  it('reads base_url from TOML', () => {
    const text = 'model = "x"\nbase_url = "https://openai.compat.test/v1"\n';
    expect(extractProviderEndpoint(text, 'toml')).toBe('https://openai.compat.test/v1');
  });
});

describe('formatEndpointHost', () => {
  it('keeps host and non-root path', () => {
    expect(formatEndpointHost('https://relay.example.com/v1')).toBe('relay.example.com/v1');
  });

  it('omits host:port from the main sentence', () => {
    expect(formatEndpointHost('https://relay.example.com:8443/v1')).toBe('relay.example.com/v1');
    expect(formatEndpointHost('http://127.0.0.1:44227')).toBe('127.0.0.1');
    expect(formatEndpointHost('http://127.0.0.1:44227')).not.toMatch(/:\d/);
  });

  it('does not cut mid-word on a long host/path', () => {
    const host = formatEndpointHost(
      'https://very-long-subdomain-name.relay.example.com/v1/compat/something-extra',
    );
    expect(host).toBe('very-long-subdomain-name.relay');
    expect(host).not.toMatch(/some$/);
    expect(host.length).toBeLessThanOrEqual(36);
    expect(host).not.toContain(':');
  });
});

describe('resolveEffectiveConnection', () => {
  it('prefers newer updatedAt when both current', () => {
    const acc = account({ updatedAt: '2026-08-01 12:00:00.000000' });
    const prov = provider({ updatedAt: '2026-08-01 13:00:00.000000' });
    const eff = resolveEffectiveConnection(acc, prov);
    expect(eff.kind).toBe('api');
    expect(eff.label).toContain('xx云中转');
    expect(eff.label).toContain('relay.example.com');
    expect(eff.authLabel).toBe('API');
  });

  it('prefers account when newer or equal', () => {
    const acc = account({ updatedAt: '2026-08-01 14:00:00.000000' });
    const prov = provider({ updatedAt: '2026-08-01 13:00:00.000000' });
    expect(resolveEffectiveConnection(acc, prov).kind).toBe('account');
    expect(resolveEffectiveConnection(acc, prov).label).toBe('me@example.com');
  });

  it('account only / provider only / none', () => {
    expect(resolveEffectiveConnection(account(), undefined).kind).toBe('account');
    expect(resolveEffectiveConnection(undefined, provider()).kind).toBe('api');
    expect(resolveEffectiveConnection(undefined, undefined)).toMatchObject({
      kind: 'none',
      label: '未配置',
      authStatus: 'none',
    });
  });

  it('marks expired account', () => {
    const eff = resolveEffectiveConnection(account({ tokenValid: false }), undefined);
    expect(eff.authStatus).toBe('expired');
  });
});

describe('formatApiConnectionLabel', () => {
  it('includes name and host', () => {
    expect(formatApiConnectionLabel(provider())).toBe('xx云中转 · relay.example.com/v1');
  });

  it('falls back to name without url', () => {
    expect(
      formatApiConnectionLabel(
        provider({ configText: 'model = "x"\n', configFormat: 'toml', name: '官方' }),
      ),
    ).toBe('官方');
  });

  it('hides adapter-generated bridge names and loopback host:port', () => {
    const generated = provider({
      id: 'claude-grok-adapter-bridge-grok-live-452e70db',
      name: 'Grok Subscription Bridge',
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'http://127.0.0.1:44227' },
      }),
    });
    expect(isInternalGeneratedProvider(generated)).toBe(true);
    expect(formatApiConnectionLabel(generated)).toBe('本机路由');
    expect(formatApiConnectionLabel(generated, { sourceLabel: 'user@x.com' })).toBe(
      '本机路由 · user@x.com',
    );
    expect(formatApiConnectionLabel(generated)).not.toMatch(/Bridge|grok-live|127\.0\.0\.1|44227/i);
    expect(formatLocalRouteLabel('user@x.com')).toBe('本机路由 · user@x.com');
  });
});

describe('applyEffectiveConnection / enrichStatusesWithConnections', () => {
  it('writes effective fields onto installed agent', () => {
    const next = applyEffectiveConnection(status(), account(), undefined);
    expect(next.effectiveKind).toBe('account');
    expect(next.effectiveLabel).toBe('me@example.com');
    expect(next.currentProvider).toBe('me@example.com');
    expect(next.authLabel).toBe('已登录');
    expect(next.authStatus).toBe('valid');
  });

  it('does not let a saved account inference overwrite live probe health', () => {
    const next = applyEffectiveConnection(
      status({ authHealth: 'verified', authSource: 'auth.json', authRevision: 'live-rev' }),
      account({ authHealth: 'renewable', refreshable: true }),
      undefined,
    );
    expect(next).toMatchObject({
      authHealth: 'verified',
      authHealthLabel: '已验证',
      authSource: 'auth.json',
      authRevision: 'live-rev',
    });
  });

  it('does not invent connection for uninstalled agents', () => {
    const next = applyEffectiveConnection(
      status({ installed: false }),
      account(),
      provider(),
    );
    expect(next.effectiveKind).toBe('none');
    expect(next.authLabel).toBe('未配置');
    expect(next.currentProvider).toBeUndefined();
  });

  it('maps current rows per agent', () => {
    const agents = [
      status({ agentId: 'claude' }),
      status({ agentId: 'codex' }),
      status({ agentId: 'kimi', installed: false }),
    ];
    const accounts = [
      account({ agentId: 'claude', isCurrent: true }),
      account({
        id: 'acc-2',
        agentId: 'codex',
        label: 'codex@x.com',
        isCurrent: false,
      }),
    ];
    const providers = [
      provider({
        agentId: 'codex',
        name: 'OpenAI 兼容',
        isCurrent: true,
        configText: 'base_url = "https://api.openai-compat.test/v1"\n',
        configFormat: 'toml',
      }),
    ];
    const enriched = enrichStatusesWithConnections(agents, accounts, providers);
    expect(enriched[0]!.effectiveKind).toBe('account');
    expect(enriched[1]!.effectiveKind).toBe('api');
    expect(enriched[1]!.effectiveLabel).toContain('OpenAI 兼容');
    expect(enriched[2]!.effectiveKind).toBe('none');
  });

  it('preserves capabilities from detect when enriching connection', () => {
    const caps = { accountSwitch: { level: 'full' as const } };
    const next = applyEffectiveConnection(
      status({ capabilities: caps }),
      account(),
      undefined,
    );
    expect(next.capabilities).toEqual(caps);
    expect(next.effectiveKind).toBe('account');
  });

  it('uses last isCurrent account when duplicates exist', () => {
    const agents = [status({ agentId: 'claude' })];
    const accounts = [
      account({ id: 'a1', label: 'first@x.com', isCurrent: true }),
      account({ id: 'a2', label: 'second@x.com', isCurrent: true }),
    ];
    const enriched = enrichStatusesWithConnections(agents, accounts, []);
    expect(enriched[0]!.effectiveLabel).toBe('second@x.com');
  });
});
