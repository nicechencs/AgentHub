import { afterEach, describe, expect, it, vi } from 'vitest';
import { getBackend } from '@/app/runtime';
import { listAccounts } from '@/lib/api/account';
import { listAdapterProfiles, planAdapter } from '@/lib/api/adapter';
import * as providerApi from '@/lib/api/provider';
import { upsertMockAccount } from '@/dev/mocks/account';
import { upsertMockProvider } from '@/dev/mocks/provider';
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

  it('wires plan / apply / listProfiles to the adapter façade', async () => {
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
    const planned = await deps.plan(request);
    const viaFacade = await planAdapter(request);
    expect(planned.canApply).toBe(true);
    expect(planned.analysis.route).toBe(viaFacade.analysis.route);

    const applied = await deps.apply(request);
    expect(applied.provider.isCurrent).toBe(true);
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
