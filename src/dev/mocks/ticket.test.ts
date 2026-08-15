import { describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import { upsertMockAccount } from './account';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';

describe('mock ticket wallet', () => {
  it('lists Kimi / Anthropic / unknown / oauth tickets and excludes generated projections', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({
      includeUnknown: true,
      includeOauthAccount: true,
      seedBindings: true,
    });

    const wallet = await getBackend().ticket.listWallet();
    const ids = wallet.tickets.map((t) => t.id).sort();
    expect(ids).toContain(`provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`);
    expect(ids).toContain(`provider:${CONNECT_FLOW_FIXTURE_IDS.anthropic}`);
    expect(ids).toContain(`provider:${CONNECT_FLOW_FIXTURE_IDS.unknownProvider}`);
    expect(ids).toContain(`account:${CONNECT_FLOW_FIXTURE_IDS.claudeOauth}`);
    expect(ids.some((id) => id.includes('claude-kimi-adapter'))).toBe(false);
    expect(ids.some((id) => id.includes('codex-kimi-bridge'))).toBe(false);

    const kimiBindings = wallet.bindings.filter(
      (b) => b.ticketId === `provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`,
    );
    expect(kimiBindings.some((b) => b.agentId === 'claude' && b.route === 'reshape')).toBe(true);
    expect(kimiBindings.some((b) => b.agentId === 'codex' && b.route === 'bridge')).toBe(true);
  });

  it('plan_ticket delegates to adapter plan for the same source', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const plan = await getBackend().ticket.plan(
      `provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`,
      'claude',
    );
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('native_endpoint');
  });

  it('includes official-login account tickets', async () => {
    getBackend();
    upsertMockAccount({
      id: 'acc-oauth',
      agentId: 'claude',
      kind: 'oauth',
      label: 'user@x.com',
      isCurrent: true,
      tokenValid: true,
    });
    const wallet = await getBackend().ticket.listWallet();
    const row = wallet.tickets.find((t) => t.id === 'account:acc-oauth');
    expect(row?.credentialClass).toBe('oauth');
    expect(
      wallet.bindings.some((b) => b.ticketId === 'account:acc-oauth' && b.route === 'native'),
    ).toBe(true);
  });
});
