import { describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import type { AdapterProfile } from '@/lib/backend/contracts';
import type { Account, Provider } from '@/lib/types';
import { upsertMockAccount } from './account';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';
import { buildMockTicketWallet } from './ticket';

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
    expect(row?.importedFrom).toBe('claude');
    expect(
      wallet.bindings.some((b) => b.ticketId === 'account:acc-oauth' && b.route === 'native'),
    ).toBe(true);
  });

  it('provider current wins over account current for the same agent', () => {
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'claude-acct',
          agentId: 'claude',
          kind: 'oauth',
          label: 'oauth',
          isCurrent: true,
          tokenValid: true,
        },
      ],
      listProviders: () => [
        {
          id: 'anth',
          agentId: 'claude',
          name: 'Anthropic',
          preset: 'anthropic',
          configText: '{}',
          configFormat: 'json',
          isCurrent: true,
        },
      ],
      listProfiles: () => [],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    const claudeActive = wallet.bindings.filter((b) => b.agentId === 'claude' && b.active);
    expect(claudeActive).toHaveLength(1);
    expect(claudeActive[0]?.ticketId).toBe('provider:anth');
    expect(claudeActive[0]?.route).toBe('native');
  });

  it('skips profile bindings when source ticket row is missing (no ghost)', () => {
    const orphanProfile: AdapterProfile = {
      id: 'orphan-p',
      name: 'Orphan',
      sourceKind: 'provider',
      sourceId: 'deleted-source',
      targetAgentId: 'claude',
      route: 'config_sync',
      mode: 'api',
      status: 'active',
      ruleId: 'kimi-membership-to-claude-v1',
      ruleVersion: '1',
      generatedProviderId: 'orphan-proj',
      localPort: null,
      autoStart: false,
      createdAt: '2026-08-15T00:00:00.000Z',
      updatedAt: '2026-08-15T00:00:00.000Z',
    };
    const wallet = buildMockTicketWallet({
      listAccounts: () => [],
      listProviders: () => [
        {
          id: 'orphan-proj',
          agentId: 'claude',
          name: 'Orphan projection',
          preset: 'custom',
          configText: '{}',
          configFormat: 'json',
          isCurrent: true,
        },
      ],
      listProfiles: () => [orphanProfile],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    expect(wallet.tickets).toEqual([]);
    expect(wallet.bindings).toEqual([]);
  });

  it('sets speaks and importedFrom lockstep with core TicketSurface rules', () => {
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'codex-oauth',
          agentId: 'codex',
          kind: 'oauth',
          label: 'me@example.com',
          isCurrent: false,
          tokenValid: true,
          credentialFormat: 'auth_json',
        },
      ],
      listProviders: () => [
        {
          id: 'kimi-src',
          agentId: 'kimi',
          name: 'Kimi',
          preset: 'kimi-code-membership',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
        },
        {
          id: 'anth',
          agentId: 'claude',
          name: 'Anthropic',
          preset: 'anthropic',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
        },
        {
          id: 'relay',
          agentId: 'claude',
          name: 'Custom relay',
          preset: 'openai-compatible',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
        },
      ],
      listProfiles: () => [],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    const kimi = wallet.tickets.find((t) => t.id === 'provider:kimi-src');
    expect(kimi?.surface).toBe('kimi-code-membership');
    expect(kimi?.speaks).toEqual(['anthropic-messages', 'openai-chat']);
    expect(kimi?.importedFrom).toBe('kimi');

    const anth = wallet.tickets.find((t) => t.id === 'provider:anth');
    expect(anth?.surface).toBe('anthropic-api');
    expect(anth?.speaks).toEqual(['anthropic-messages']);
    expect(anth?.importedFrom).toBe('claude');

    const codex = wallet.tickets.find((t) => t.id === 'account:codex-oauth');
    expect(codex?.surface).toBe('codex-chatgpt-subscription');
    expect(codex?.speaks).toEqual(['openai-responses']);
    expect(codex?.importedFrom).toBe('codex');

    const relay = wallet.tickets.find((t) => t.id === 'provider:relay');
    expect(relay?.surface).toBe('unknown');
    expect(relay?.speaks).toEqual([]);
    expect(relay?.credentialClass).toBe('api_key');
    expect(relay?.importedFrom).toBe('claude');
  });

  it('uses persisted extra.surface / meta.surface when fixture provides them', () => {
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'stamped-acct',
          agentId: 'grok',
          kind: 'apikey',
          label: 'xai',
          isCurrent: false,
          tokenValid: true,
          extra: { surface: 'anthropic-api' },
        } as Account,
      ],
      listProviders: () => [
        {
          id: 'stamped-prov',
          agentId: 'claude',
          name: 'Custom but stamped',
          preset: 'openai-compatible',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
          meta: { surface: 'kimi-code-membership' },
        } as Provider,
      ],
      listProfiles: () => [],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    expect(wallet.tickets.find((t) => t.id === 'account:stamped-acct')?.surface).toBe(
      'anthropic-api',
    );
    expect(wallet.tickets.find((t) => t.id === 'provider:stamped-prov')?.surface).toBe(
      'kimi-code-membership',
    );
    expect(wallet.tickets.find((t) => t.id === 'provider:stamped-prov')?.speaks).toEqual([
      'anthropic-messages',
      'openai-chat',
    ]);
  });

  it('plan_ticket rejects generated projection providers', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ seedBindings: true });
    const generatedId = `claude-kimi-adapter-${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`;
    await expect(getBackend().ticket.plan(`provider:${generatedId}`, 'pi')).rejects.toMatchObject({
      code: 'invalid_arg',
      message: expect.stringContaining('投影不是票'),
    });
  });

  it('bind_ticket reuses apply and returns the active binding for the target Agent', async () => {
    getBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const ticketId = `provider:${kimiMembership.id}`;
    const { binding } = await getBackend().ticket.bind(ticketId, 'pi');
    expect(binding.active).toBe(true);
    expect(binding.agentId).toBe('pi');
    expect(binding.ticketId).toBe(ticketId);
    expect(binding.route).toBe('reshape');
    const wallet = await getBackend().ticket.listWallet();
    expect(wallet.bindings.some((row) => (
      row.active && row.agentId === 'pi' && row.ticketId === ticketId
    ))).toBe(true);
  });

  it('bind_ticket allows Account Anthropic → Pi and rejects generated projections', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeAnthropic: false, seedBindings: true });
    upsertMockAccount({
      id: 'anth-acc-bind',
      agentId: 'claude',
      kind: 'apikey',
      label: 'Anthropic key',
      isCurrent: false,
      tokenValid: true,
      extra: { provider: 'anthropic' },
    } as Account);
    const { binding } = await getBackend().ticket.bind('account:anth-acc-bind', 'pi');
    expect(binding.active).toBe(true);
    expect(binding.agentId).toBe('pi');
    expect(binding.ticketId).toBe('account:anth-acc-bind');

    const generatedId = `claude-kimi-adapter-${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`;
    await expect(getBackend().ticket.bind(`provider:${generatedId}`, 'pi')).rejects.toMatchObject({
      code: 'invalid_arg',
      message: expect.stringContaining('投影不是票'),
    });
  });

  it('unbind_ticket removes the binding even when the projection is current', async () => {
    getBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const ticketId = `provider:${kimiMembership.id}`;
    const { binding } = await getBackend().ticket.bind(ticketId, 'pi');
    expect(binding.active).toBe(true);
    await getBackend().ticket.unbind(ticketId, 'pi');
    const wallet = await getBackend().ticket.listWallet();
    expect(wallet.tickets.some((row) => row.id === ticketId)).toBe(true);
    expect(wallet.bindings.some((row) => row.ticketId === ticketId && row.agentId === 'pi')).toBe(false);
  });
});
