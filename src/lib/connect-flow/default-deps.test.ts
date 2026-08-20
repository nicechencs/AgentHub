import { afterEach, describe, expect, it, vi } from 'vitest';
import { getBackend } from '@/app/runtime';
import { listAccounts } from '@/lib/api/account';
import * as adapterApi from '@/lib/api/adapter';
import { listAdapterProfiles, planAdapter } from '@/lib/api/adapter';
import * as providerApi from '@/lib/api/provider';
import * as ticketsApi from '@/lib/api/tickets';
import { listTicketWallet } from '@/lib/api/tickets';
import { upsertMockAccount } from '@/dev/mocks/account';
import { seedConnectFlowAdapterFixtures } from '@/dev/mocks/connect-flow-fixtures';
import { createMockProviderPort, upsertMockProvider } from '@/dev/mocks/provider';
import { createDefaultConnectFlowDeps } from './default-deps';
import { planFanoutKey, type SourceOption } from './types';

function seedAfterBackend(): void {
  // createBackend() resets account/provider mocks; materialize first, then seed.
  getBackend();
}

function providerOption(id: string, label: string): SourceOption {
  return {
    ref: { kind: 'provider', id },
    group: 'native',
    agentId: 'claude',
    label,
    state: { kind: 'switchable' },
  };
}

describe('createDefaultConnectFlowDeps', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('keeps the default mock pool empty until ConnectFlow adapter fixtures are seeded', async () => {
    seedAfterBackend();
    expect(await createMockProviderPort().listProviders()).toEqual([]);
    const seeded = seedConnectFlowAdapterFixtures();
    const providers = await createMockProviderPort().listProviders();
    expect(providers.map((item) => item.id).sort()).toEqual(
      [seeded.kimiMembership.id, seeded.anthropic!.id].sort(),
    );
  });

  it('plans and binds Kimi membership → installed Pi through the ticket façade', async () => {
    seedAfterBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const deps = createDefaultConnectFlowDeps();
    const request = {
      sourceKind: 'provider' as const,
      sourceId: kimiMembership.id,
      targetAgentId: 'pi' as const,
    };
    const bindSpy = vi.spyOn(ticketsApi, 'bindTicket');
    const applySpy = vi.spyOn(adapterApi, 'applyAdapter');
    const planned = await deps.plan(request);
    const viaFacade = await planAdapter(request);
    expect(planned.canApply).toBe(true);
    expect(viaFacade.canApply).toBe(true);
    expect(planned.analysis.route).toBe('config_sync');

    const applied = await deps.apply(request);
    expect(bindSpy).toHaveBeenCalledWith(`provider:${kimiMembership.id}`, 'pi');
    expect(applySpy).not.toHaveBeenCalled();
    expect(applied.profile.route).toBe('config_sync');
    expect(applied.provider.agentId).toBe('pi');
    const wallet = await listTicketWallet();
    expect(wallet.bindings.some((row) => (
      row.active && row.agentId === 'pi' && row.ticketId === `provider:${kimiMembership.id}`
    ))).toBe(true);
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');
  });

  it('plans and binds Anthropic API → installed Pi through the ticket façade', async () => {
    seedAfterBackend();
    const { anthropic } = seedConnectFlowAdapterFixtures();
    expect(anthropic).toBeDefined();
    const deps = createDefaultConnectFlowDeps();
    const request = {
      sourceKind: 'provider' as const,
      sourceId: anthropic!.id,
      targetAgentId: 'pi' as const,
    };
    const bindSpy = vi.spyOn(ticketsApi, 'bindTicket');
    const applySpy = vi.spyOn(adapterApi, 'applyAdapter');
    const planned = await deps.plan(request);
    expect(planned.canApply).toBe(true);
    expect(planned.analysis.route).toBe('config_sync');
    expect(planned.analysis.ruleId).toBe('anthropic-api-to-pi-v1');

    const applied = await deps.apply(request);
    expect(bindSpy).toHaveBeenCalledWith(`provider:${anthropic!.id}`, 'pi');
    expect(applySpy).not.toHaveBeenCalled();
    expect(applied.profile.route).toBe('config_sync');
    expect(applied.provider.agentId).toBe('pi');
    const wallet = await listTicketWallet();
    expect(wallet.bindings.some((row) => (
      row.active && row.agentId === 'pi' && row.ticketId === `provider:${anthropic!.id}`
    ))).toBe(true);
    expect(JSON.parse(applied.provider.configText)).toEqual({
      slot: 'anthropic',
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');
  });

  it('wires plan / bind / listProfiles to the ticket façade', async () => {
    seedAfterBackend();
    upsertMockProvider({
      id: 'kimi-member',
      agentId: 'kimi',
      name: 'Kimi membership',
      preset: 'kimi-code-membership',
      configText: 'api_key = "secret"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const deps = createDefaultConnectFlowDeps();
    const request = {
      sourceKind: 'provider' as const,
      sourceId: 'kimi-member',
      targetAgentId: 'claude' as const,
    };
    const bindSpy = vi.spyOn(ticketsApi, 'bindTicket');
    const applySpy = vi.spyOn(adapterApi, 'applyAdapter');
    const planned = await deps.plan(request);
    const viaFacade = await planAdapter(request);
    expect(planned.canApply).toBe(true);
    expect(planned.analysis.route).toBe(viaFacade.analysis.route);

    const applied = await deps.apply(request);
    expect(bindSpy).toHaveBeenCalledWith('provider:kimi-member', 'claude');
    expect(applySpy).not.toHaveBeenCalled();
    const wallet = await listTicketWallet();
    expect(wallet.bindings.some((row) => (
      row.active && row.agentId === 'claude' && row.ticketId === 'provider:kimi-member'
    ))).toBe(true);
    expect(applied.profile.generatedProviderId).toBe(applied.provider.id);

    const profiles = await deps.listProfiles();
    const listed = await listAdapterProfiles();
    expect(profiles).toHaveLength(listed.length);
    expect(profiles[0]?.id).toBe(applied.profile.id);
  });

  it('switchNative for accounts calls the same switchAccount façade as Connections', async () => {
    seedAfterBackend();
    upsertMockAccount({
      id: 'acc-a',
      agentId: 'claude',
      kind: 'oauth',
      label: 'a@claude',
      isCurrent: true,
      tokenValid: true,
    });
    upsertMockAccount({
      id: 'acc-b',
      agentId: 'claude',
      kind: 'oauth',
      label: 'b@claude',
      isCurrent: false,
      tokenValid: true,
    });
    const deps = createDefaultConnectFlowDeps();
    await deps.switchNative({
      ref: { kind: 'account', id: 'acc-b' },
      group: 'native',
      agentId: 'claude',
      label: 'b@claude',
      state: { kind: 'switchable' },
    });
    const accounts = await listAccounts('claude');
    expect(accounts.find((item) => item.id === 'acc-b')?.isCurrent).toBe(true);
    expect(accounts.find((item) => item.id === 'acc-a')?.isCurrent).toBe(false);
  });

  it('switchNative for providers previews then switches (Connections chain)', async () => {
    seedAfterBackend();
    upsertMockProvider({
      id: 'p-a',
      agentId: 'claude',
      name: 'A',
      preset: 'anthropic',
      configText: '{}',
      configFormat: 'json',
      isCurrent: true,
    });
    upsertMockProvider({
      id: 'p-b',
      agentId: 'claude',
      name: 'B',
      preset: 'anthropic',
      configText: '{}',
      configFormat: 'json',
      isCurrent: false,
    });
    const previewSpy = vi.spyOn(providerApi, 'switchPreview');
    const switchSpy = vi.spyOn(providerApi, 'switchProvider');
    const deps = createDefaultConnectFlowDeps();
    await deps.switchNative(providerOption('p-b', 'B'));
    expect(previewSpy).toHaveBeenCalledTimes(1);
    expect(switchSpy).toHaveBeenCalledTimes(1);
    expect(previewSpy.mock.invocationCallOrder[0]).toBeLessThan(switchSpy.mock.invocationCallOrder[0]!);
    expect(previewSpy).toHaveBeenCalledWith('claude', 'p-b');
    expect(switchSpy).toHaveBeenCalledWith('claude', 'p-b');
    const providers = await providerApi.listProviders('claude');
    expect(providers.find((item) => item.id === 'p-b')?.isCurrent).toBe(true);
  });

  it('does not call switchProvider when switchPreview fails', async () => {
    seedAfterBackend();
    upsertMockProvider({
      id: 'p-a',
      agentId: 'claude',
      name: 'A',
      preset: 'anthropic',
      configText: '{}',
      configFormat: 'json',
      isCurrent: true,
    });
    upsertMockProvider({
      id: 'p-b',
      agentId: 'claude',
      name: 'B',
      preset: 'anthropic',
      configText: '{}',
      configFormat: 'json',
      isCurrent: false,
    });
    vi.spyOn(providerApi, 'switchPreview').mockRejectedValue(new Error('preview failed'));
    const switchSpy = vi.spyOn(providerApi, 'switchProvider');
    const deps = createDefaultConnectFlowDeps();
    await expect(deps.switchNative(providerOption('p-b', 'B'))).rejects.toThrow('preview failed');
    expect(switchSpy).not.toHaveBeenCalled();
  });

  it('previewNative for providers calls switchPreview and does not switch', async () => {
    seedAfterBackend();
    const preview = {
      backfillSummary: '当前生效配置将回存为「A」',
      backupPath: '~/.agenthub/backups/live/claude/',
    };
    const previewSpy = vi.spyOn(providerApi, 'switchPreview').mockResolvedValue(preview);
    const switchSpy = vi.spyOn(providerApi, 'switchProvider');
    const deps = createDefaultConnectFlowDeps();
    expect(deps.previewNative).toEqual(expect.any(Function));
    await expect(deps.previewNative!(providerOption('p-b', 'B'))).resolves.toEqual(preview);
    expect(previewSpy).toHaveBeenCalledTimes(1);
    expect(previewSpy).toHaveBeenCalledWith('claude', 'p-b');
    expect(switchSpy).not.toHaveBeenCalled();
  });

  it('previewNative for accounts returns null without calling switchPreview', async () => {
    seedAfterBackend();
    const previewSpy = vi.spyOn(providerApi, 'switchPreview');
    const deps = createDefaultConnectFlowDeps();
    await expect(deps.previewNative!({
      ref: { kind: 'account', id: 'acc-b' },
      group: 'native',
      agentId: 'claude',
      label: 'b@claude',
      state: { kind: 'switchable' },
    })).resolves.toBeNull();
    expect(previewSpy).not.toHaveBeenCalled();
  });

  it('apply accepts official Codex native self-bind without a generated provider', async () => {
    seedAfterBackend();
    vi.spyOn(ticketsApi, 'bindTicket').mockResolvedValue({
      binding: {
        ticketId: 'account:codex-live-1',
        agentId: 'codex',
        route: 'native',
        active: true,
        profileId: null,
        bridge: null,
      },
    });
    const listProfiles = vi.spyOn(adapterApi, 'listAdapterProfiles');
    const deps = createDefaultConnectFlowDeps();
    const result = await deps.apply({
      sourceKind: 'account',
      sourceId: 'codex-live-1',
      targetAgentId: 'codex',
    });
    expect(result.profile.route).toBe('native_endpoint');
    expect(result.profile.generatedProviderId).toBeNull();
    expect(result.provider.name).toBe('官方登录');
    expect(result.provider.name).not.toBe('本机路由');
    expect(listProfiles).not.toHaveBeenCalled();
  });

  it('createPlanFanout uses the wired plan and OAuth precheck', async () => {
    seedAfterBackend();
    upsertMockAccount({
      id: 'oauth-incomplete',
      agentId: 'codex',
      kind: 'oauth',
      label: 'codex@openai',
      isCurrent: false,
      tokenValid: false,
      authHealth: 'needs_login',
    });
    const deps = createDefaultConnectFlowDeps();
    const fanout = deps.createPlanFanout();
    const req = {
      source: { kind: 'account' as const, id: 'oauth-incomplete' },
      targetAgentId: 'claude' as const,
    };
    fanout.start([req], {
      accounts: await listAccounts('codex'),
    });
    await Promise.resolve();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('blocked_oauth');
  });
});
