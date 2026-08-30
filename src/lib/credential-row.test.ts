import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { Account, Provider } from '@/lib/types';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import { accountEndpointExtras, providerEndpointMode, toCredentialRow } from './credential-row';

function acc(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'claude',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function prov(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'claude',
    preset: 'custom',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com',
        ANTHROPIC_AUTH_TOKEN: '***',
      },
    }),
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

function ticket(partial: Partial<TicketView> & Pick<TicketView, 'id' | 'sourceKind' | 'sourceId' | 'label'>): TicketView {
  return {
    agentId: 'claude',
    surface: 'anthropic-api',
    credentialClass: 'api_key',
    speaks: [],
    importedFrom: null,
    ...partial,
  };
}

describe('toCredentialRow', () => {
  it('projects account stable fields and auth summary', () => {
    const row = toCredentialRow({
      source: 'account',
      account: acc({ id: 'a1', kind: 'oauth', label: 'me@x.com', isCurrent: true, subscription: 'Pro' }),
    });
    expect(row).toMatchObject({
      key: 'account:a1',
      source: 'account',
      id: 'a1',
      agentId: 'claude',
      title: 'me@x.com',
      isCurrent: true,
    });
    expect(row.subtitle).toContain('Pro');
    expect(row.auth.label).toBeTruthy();
    expect(row.auth.status).toBeTruthy();
  });

  it('hides adapter-generated bridge names from the title', () => {
    const row = toCredentialRow({
      source: 'provider',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-1',
        name: 'Grok Subscription Bridge',
        configText: JSON.stringify({
          env: { ANTHROPIC_BASE_URL: 'http://127.0.0.1:44227' },
        }),
      }),
    });
    expect(row.title).toBe('本机路由');
    expect(row.title).not.toMatch(/Bridge|grok-live|127\.0\.0\.1/i);
    expect(row.subtitle).not.toMatch(/127\.0\.0\.1|44227/);
  });

  it('projects provider stable fields with endpoint subtitle', () => {
    const row = toCredentialRow({
      source: 'provider',
      provider: prov({ id: 'p1', name: 'Relay' }),
    });
    expect(row).toMatchObject({
      key: 'provider:p1',
      source: 'provider',
      id: 'p1',
      title: 'Relay',
      isCurrent: false,
    });
    expect(row.subtitle).toContain('自定义端点');
    expect(row.auth.health).toBe('configured');
  });

  it('projects ticket along the same axis', () => {
    const row = toCredentialRow({
      source: 'ticket',
      ticket: ticket({
        id: 'account:t1',
        sourceKind: 'account',
        sourceId: 't1',
        label: 'Claude OAuth',
        credentialClass: 'oauth',
        surface: 'claude-subscription',
      }),
      isCurrent: true,
    });
    expect(row).toMatchObject({
      key: 'account:t1',
      source: 'account',
      id: 't1',
      title: 'Claude OAuth',
      isCurrent: true,
    });
    expect(row.subtitle).toContain('Official login');
  });

  it('translates the ticket credential/surface subtitle when a translator is passed', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const input = {
      source: 'ticket' as const,
      ticket: ticket({
        id: 'account:t2',
        sourceKind: 'account' as const,
        sourceId: 't2',
        label: 'Claude OAuth',
        credentialClass: 'oauth' as const,
        surface: 'claude-subscription' as const,
      }),
      isCurrent: true,
    };

    const zhRow = toCredentialRow(input, tZh);
    expect(zhRow.subtitle).toContain('官方登录');
    expect(zhRow.auth.label).toBe('官方登录');

    const enRow = toCredentialRow(input, tEn);
    expect(enRow.subtitle).toContain('Official login');
    expect(enRow.auth.label).toBe('Official login');
  });

  it('translates the account idle subtitle when a translator is passed', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const account = acc({ id: 'a2', kind: 'oauth', label: 'me@x.com', isCurrent: false });

    const zhRow = toCredentialRow({ source: 'account', account }, tZh);
    expect(zhRow.subtitle).toContain('未生效');

    const enRow = toCredentialRow({ source: 'account', account }, tEn);
    expect(enRow.subtitle).toContain('Not current');
    expect(enRow.subtitle).not.toContain('未生效');
    expect(enRow.subtitle).not.toMatch(/[\u4e00-\u9fff]/);
    expect(enRow.auth.label).not.toMatch(/[\u4e00-\u9fff]/);
  });

  it('translates generated local-route titles when a translator is passed', () => {
    const tEn = createTranslator('en');
    const row = toCredentialRow({
      source: 'provider',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-1',
        name: 'Grok Subscription Bridge',
        configText: JSON.stringify({
          env: { ANTHROPIC_BASE_URL: 'http://127.0.0.1:44227' },
        }),
      }),
    }, tEn);
    expect(row.title).toBe('Local route');
    expect(row.title).not.toMatch(/[\u4e00-\u9fff]/);
  });

  it('translates the provider endpoint/current subtitle when a translator is passed', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const provider = prov({ id: 'p2', name: 'Relay', isCurrent: true });

    const zhRow = toCredentialRow({ source: 'provider', provider }, tZh);
    expect(zhRow.subtitle).toContain('自定义端点');
    expect(zhRow.subtitle).toContain('当前生效');

    const enRow = toCredentialRow({ source: 'provider', provider }, tEn);
    expect(enRow.subtitle).toContain('Custom endpoint');
    expect(enRow.subtitle).toContain('current');
    expect(enRow.subtitle).not.toMatch(/[\u4e00-\u9fff]/);
  });

  it('falls back to Chinese literals when no translator is passed (backward compat)', () => {
    const account = acc({ id: 'a3', kind: 'oauth', label: 'me@x.com', isCurrent: false });
    const row = toCredentialRow({ source: 'account', account });
    expect(row.subtitle).toContain('未生效');
  });
});

describe('providerEndpointMode', () => {
  it('treats official=false as custom even when preset looks official', () => {
    const provider = prov({
      id: 'p-custom',
      name: 'Relay',
      preset: 'anthropic',
      official: false,
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'https://relay.example.com' },
      }),
    });
    expect(providerEndpointMode(provider, 'https://relay.example.com')).toBe('custom');
  });

  it('treats official=true as official', () => {
    const provider = prov({
      id: 'p-official',
      name: 'Official',
      preset: 'anthropic',
      official: true,
      configText: '{}',
    });
    expect(providerEndpointMode(provider, '')).toBe('official');
  });

  it('falls back to URL when official is unset', () => {
    const custom = prov({
      id: 'p-url',
      name: 'Relay',
      preset: 'custom',
      configText: JSON.stringify({
        env: { ANTHROPIC_BASE_URL: 'https://relay.example.com' },
      }),
    });
    expect(providerEndpointMode(custom, 'https://relay.example.com')).toBe('custom');

    const emptyOfficial = prov({
      id: 'p-empty',
      name: 'Claude',
      preset: 'anthropic',
      configText: '{}',
    });
    expect(providerEndpointMode(emptyOfficial, '')).toBe('official');
  });
});

describe('accountEndpointExtras', () => {
  it('keeps API keys without a URL on the legacy official mode', () => {
    expect(accountEndpointExtras(acc({ id: 'a1', kind: 'apikey', label: 'Key' }))).toEqual({
      endpointMode: 'official',
    });
  });

  it('marks a ZCode catalog URL that is not Z.ai as custom', () => {
    const extras = accountEndpointExtras(acc({
      id: 'z1',
      agentId: 'zcode',
      kind: 'apikey',
      label: 'grok',
      endpoint: 'https://api.qooo.io/v1',
    }));
    expect(extras.endpointMode).toBe('custom');
    expect(extras.endpoint).toBe('https://api.qooo.io/v1');
    expect(extras.endpointHost).toContain('api.qooo.io');
  });
});
