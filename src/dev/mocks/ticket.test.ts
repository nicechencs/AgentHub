import { describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import type { AdapterProfile } from '@/lib/backend/contracts';
import type { Account, Provider } from '@/lib/types';
import { upsertMockAccount } from './account';
import { upsertMockProvider } from './provider';
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
    expect(ids).toContain(`account:${CONNECT_FLOW_FIXTURE_IDS.kimiMembershipStale}`);
    expect(ids.some((id) => id.includes('claude-kimi-adapter'))).toBe(false);
    expect(ids.some((id) => id.includes('codex-kimi-bridge'))).toBe(false);

    const kimiBindings = wallet.bindings.filter(
      (b) => b.ticketId === `provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`,
    );
    expect(kimiBindings.some((b) => b.agentId === 'claude' && b.route === 'reshape')).toBe(true);
    expect(kimiBindings.some((b) => b.agentId === 'codex' && b.route === 'bridge')).toBe(true);
  });

  it('seeds a two-member kimi pool with one NeedsLogin member', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ seedBindings: true });
    const wallet = await getBackend().ticket.listWallet();
    const kimi = wallet.surfaceGroups.find(
      (group) => group.surface === 'kimi-code-membership' && group.credentialClass === 'api_key',
    );
    expect(kimi?.members.map((member) => member.ticketId).sort()).toEqual([
      `account:${CONNECT_FLOW_FIXTURE_IDS.kimiMembershipStale}`,
      `provider:${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`,
    ]);
    expect(kimi?.members.find((member) => member.sourceId === CONNECT_FLOW_FIXTURE_IDS.kimiMembership)?.health)
      .toBe('renewable');
    expect(kimi?.members.find((member) => member.sourceId === CONNECT_FLOW_FIXTURE_IDS.kimiMembershipStale)?.health)
      .toBe('needs_login');
    expect(kimi?.members.find((member) => member.health === 'needs_login')?.label)
      .toBe('Kimi 会员（失效号）');
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
    expect(wallet.surfaceGroups).toEqual([]);
  });

  it('groups same surface+class, mixes account/provider, skips unknown and projections', () => {
    const generated: AdapterProfile = {
      id: 'proj-p',
      name: 'Generated',
      sourceKind: 'provider',
      sourceId: 'kimi-a',
      targetAgentId: 'claude',
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: 'kimi-membership-to-claude-v1',
      ruleVersion: '1',
      generatedProviderId: 'proj-claude',
      localPort: null,
      autoStart: false,
      createdAt: '2026-08-15T00:00:00.000Z',
      updatedAt: '2026-08-15T00:00:00.000Z',
    };
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'kimi-key',
          agentId: 'kimi',
          kind: 'apikey',
          label: 'Kimi key',
          isCurrent: false,
          tokenValid: true,
          extra: { surface: 'kimi-code-membership' },
        } as Account,
        {
          id: 'grok-a',
          agentId: 'grok',
          kind: 'oauth',
          label: 'a@x.com',
          isCurrent: false,
          tokenValid: true,
        },
        {
          id: 'grok-b',
          agentId: 'grok',
          kind: 'oauth',
          label: 'b@x.com',
          isCurrent: false,
          tokenValid: true,
        },
      ],
      listProviders: () => [
        {
          id: 'kimi-a',
          agentId: 'kimi',
          name: 'Kimi membership',
          preset: 'kimi-code-membership',
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
        {
          id: 'proj-claude',
          agentId: 'claude',
          name: 'Generated',
          preset: 'custom',
          configText: '{}',
          configFormat: 'json',
          isCurrent: true,
        },
      ],
      listProfiles: () => [generated],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    expect(wallet.tickets.some((t) => t.id === 'provider:proj-claude')).toBe(false);
    const kimi = wallet.surfaceGroups.find(
      (g) => g.surface === 'kimi-code-membership' && g.credentialClass === 'api_key',
    );
    expect(kimi?.members.map((m) => m.ticketId)).toEqual([
      'account:kimi-key',
      'provider:kimi-a',
    ]);
    const grok = wallet.surfaceGroups.find(
      (g) => g.surface === 'grok-xai-subscription' && g.credentialClass === 'oauth',
    );
    expect(grok?.members.map((m) => m.ticketId)).toEqual([
      'account:grok-a',
      'account:grok-b',
    ]);
    expect(wallet.surfaceGroups.some((g) => g.surface === 'unknown')).toBe(false);
    expect(wallet.tickets.some((t) => t.surface === 'unknown')).toBe(true);
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
        {
          id: 'claude-oauth',
          agentId: 'claude',
          kind: 'oauth',
          label: 'Claude subscription',
          isCurrent: false,
          tokenValid: true,
        },
        {
          id: 'grok-oauth',
          agentId: 'grok',
          kind: 'oauth',
          label: 'Grok subscription',
          isCurrent: false,
          tokenValid: true,
        },
        {
          id: 'pi-oauth',
          agentId: 'pi',
          kind: 'oauth',
          label: 'Pi OAuth',
          isCurrent: false,
          tokenValid: true,
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
    expect(codex?.speaks).toEqual(['openai-responses', 'openai-codex-pkce']);
    expect(codex?.importedFrom).toBe('codex');

    const claude = wallet.tickets.find((t) => t.id === 'account:claude-oauth');
    expect(claude?.surface).toBe('claude-subscription');
    expect(claude?.speaks).toEqual(['anthropic-messages', 'anthropic-pkce']);
    expect(claude?.importedFrom).toBe('claude');

    const grok = wallet.tickets.find((t) => t.id === 'account:grok-oauth');
    expect(grok?.surface).toBe('grok-xai-subscription');
    expect(grok?.speaks).toEqual(['openai-responses', 'openai-chat', 'xai-device-code']);
    expect(grok?.importedFrom).toBe('grok');

    const pi = wallet.tickets.find((t) => t.id === 'account:pi-oauth');
    expect(pi?.surface).toBe('unknown');
    expect(pi?.speaks).toEqual([]);
    expect(pi?.importedFrom).toBe('pi');

    const relay = wallet.tickets.find((t) => t.id === 'provider:relay');
    expect(relay?.surface).toBe('unknown');
    expect(relay?.speaks).toEqual([]);
    expect(relay?.credentialClass).toBe('api_key');
    expect(relay?.importedFrom).toBe('claude');
  });

  it('classifies openai/xai/glm/deepseek by explicit markers and leaves custom relays unknown', () => {
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'openai-acc',
          agentId: 'codex',
          kind: 'apikey',
          label: 'OpenAI',
          isCurrent: false,
          tokenValid: true,
          extra: { provider: 'openai' },
        } as Account,
        {
          id: 'glm-acc',
          agentId: 'claude',
          kind: 'apikey',
          label: 'GLM',
          isCurrent: false,
          tokenValid: true,
          extra: { provider: 'glm-coding-plan' },
        } as Account,
      ],
      listProviders: () => [
        {
          id: 'openai-src',
          agentId: 'codex',
          name: 'OpenAI',
          preset: 'openai',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
        },
        {
          id: 'xai-host',
          agentId: 'grok',
          name: 'xAI host',
          preset: 'custom',
          configText: '{"baseUrl":"https://api.x.ai/v1"}',
          configFormat: 'json',
          isCurrent: false,
        },
        {
          id: 'deepseek-src',
          agentId: 'claude',
          name: 'DeepSeek',
          preset: 'deepseek-api',
          configText: '{}',
          configFormat: 'json',
          isCurrent: false,
        },
        {
          id: 'relay',
          agentId: 'claude',
          name: 'Custom relay',
          preset: 'openai-compatible',
          configText: '{"baseUrl":"https://relay.example/v1"}',
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

    expect(wallet.tickets.find((t) => t.id === 'provider:openai-src')).toMatchObject({
      surface: 'openai-api',
      speaks: ['openai-chat'],
    });
    expect(wallet.tickets.find((t) => t.id === 'account:openai-acc')).toMatchObject({
      surface: 'openai-api',
      speaks: ['openai-chat'],
    });
    expect(wallet.tickets.find((t) => t.id === 'provider:xai-host')).toMatchObject({
      surface: 'xai-api',
      speaks: ['openai-responses', 'openai-chat'],
    });
    expect(wallet.tickets.find((t) => t.id === 'account:glm-acc')).toMatchObject({
      surface: 'glm-coding-plan',
      speaks: ['anthropic-messages', 'openai-chat'],
    });
    expect(wallet.tickets.find((t) => t.id === 'provider:deepseek-src')).toMatchObject({
      surface: 'deepseek-api',
      speaks: ['anthropic-messages', 'openai-chat'],
    });
    expect(wallet.tickets.find((t) => t.id === 'provider:relay')?.surface).toBe('unknown');
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

  it('reclassifies persisted unknown OAuth surfaces without writing unknown back', () => {
    const wallet = buildMockTicketWallet({
      listAccounts: () => [
        {
          id: 'unknown-claude',
          agentId: 'claude',
          kind: 'oauth',
          label: 'Claude OAuth',
          isCurrent: false,
          tokenValid: true,
          extra: { surface: 'unknown' },
        } as Account,
        {
          id: 'unknown-pi',
          agentId: 'pi',
          kind: 'oauth',
          label: 'Pi OAuth',
          isCurrent: false,
          tokenValid: true,
          extra: { surface: 'unknown' },
        } as Account,
      ],
      listProviders: () => [],
      listProfiles: () => [],
      getBridgeStatus: () => undefined,
      planAdapter: async () => {
        throw new Error('not used');
      },
    });

    expect(wallet.tickets.find((t) => t.id === 'account:unknown-claude')?.surface)
      .toBe('claude-subscription');
    expect(wallet.tickets.find((t) => t.id === 'account:unknown-pi')?.surface).toBe('unknown');
  });

  it('plan_ticket rejects generated projection providers', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ seedBindings: true });
    const generatedId = `claude-kimi-adapter-${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`;
    await expect(getBackend().ticket.plan(`provider:${generatedId}`, 'pi')).rejects.toMatchObject({
      code: 'invalid_arg',
      message: expect.stringContaining('自动生成的配置不是登录'),
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
      message: expect.stringContaining('自动生成的配置不是登录'),
    });
  });

  it('bind_ticket allows OpenAI / xAI Provider+Account → Pi and rejects unknown relays', async () => {
    getBackend();
    upsertMockAccount({
      id: 'openai-acc-bind',
      agentId: 'codex',
      kind: 'apikey',
      label: 'OpenAI key',
      isCurrent: false,
      tokenValid: true,
      extra: { provider: 'openai' },
    } as Account);
    const { binding } = await getBackend().ticket.bind('account:openai-acc-bind', 'pi');
    expect(binding.active).toBe(true);
    expect(binding.agentId).toBe('pi');
    expect(binding.route).toBe('reshape');

    upsertMockProvider({
      id: 'xai-prov-bind',
      agentId: 'grok',
      name: 'xAI',
      preset: 'xai',
      configText: '{}',
      configFormat: 'json',
      isCurrent: false,
    });
    const xaiBind = await getBackend().ticket.bind('provider:xai-prov-bind', 'pi');
    expect(xaiBind.binding.active).toBe(true);
    await getBackend().ticket.unbind('provider:xai-prov-bind', 'pi');

    upsertMockProvider({
      id: 'relay-no-bind',
      agentId: 'claude',
      name: 'Relay',
      preset: 'openai-compatible',
      configText: '{"baseUrl":"https://relay.example/v1"}',
      configFormat: 'json',
      isCurrent: false,
    });
    await expect(getBackend().ticket.bind('provider:relay-no-bind', 'pi')).rejects.toMatchObject({
      code: 'unsupported',
    });
  });

  it('plan/bind GLM Provider and DeepSeek Account → Claude, and rejects unknown relays', async () => {
    getBackend();
    upsertMockProvider({
      id: 'glm-prov-bind',
      agentId: 'claude',
      name: 'GLM',
      preset: 'glm-coding-plan',
      configText: '{}',
      configFormat: 'json',
      isCurrent: false,
    });
    const glmPlan = await getBackend().ticket.plan('provider:glm-prov-bind', 'claude');
    expect(glmPlan.canApply).toBe(true);
    expect(glmPlan.analysis.ruleId).toBe('glm-coding-plan-to-claude-v1');
    expect(glmPlan.changes[0].value).toBe('https://open.bigmodel.cn/api/anthropic');
    const glmBind = await getBackend().ticket.bind('provider:glm-prov-bind', 'claude');
    expect(glmBind.binding.active).toBe(true);
    expect(glmBind.binding.route).toBe('reshape');
    await getBackend().ticket.unbind('provider:glm-prov-bind', 'claude');

    upsertMockAccount({
      id: 'deepseek-acc-bind',
      agentId: 'claude',
      kind: 'apikey',
      label: 'DeepSeek',
      isCurrent: false,
      tokenValid: true,
      extra: { provider: 'deepseek-api' },
      credentials: { format: 'api_key', api_key: 'sk-deepseek' },
    } as Account);
    const dsPlan = await getBackend().ticket.plan('account:deepseek-acc-bind', 'claude');
    expect(dsPlan.canApply).toBe(true);
    expect(dsPlan.analysis.ruleId).toBe('deepseek-api-to-claude-v1');
    const dsBind = await getBackend().ticket.bind('account:deepseek-acc-bind', 'claude');
    expect(dsBind.binding.active).toBe(true);
    expect(dsBind.binding.ticketId).toBe('account:deepseek-acc-bind');

    upsertMockProvider({
      id: 'kimi-still-kimi-url',
      agentId: 'kimi',
      name: 'Kimi',
      preset: 'kimi-code-membership',
      configText: 'api_key = "kimi-secret"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const kimiPlan = await getBackend().ticket.plan('provider:kimi-still-kimi-url', 'claude');
    expect(kimiPlan.canApply).toBe(true);
    expect(kimiPlan.changes[0].value).toBe('https://api.kimi.com/coding/');

    upsertMockProvider({
      id: 'relay-no-claude',
      agentId: 'claude',
      name: 'Relay',
      preset: 'openai-compatible',
      configText: '{"baseUrl":"https://relay.example/v1"}',
      configFormat: 'json',
      isCurrent: false,
    });
    await expect(getBackend().ticket.bind('provider:relay-no-claude', 'claude')).rejects.toMatchObject({
      code: 'unsupported',
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
