import { beforeEach, describe, expect, it } from 'vitest';
import { createMockAdapterPort, resetMockAdapters } from './adapter';
import { getMockAccountById } from './account';
import {
  createMockProviderPort,
  getMockProviderById,
  removeMockProvider,
  resetMockProviders,
  upsertMockProvider,
} from './provider';

describe('mock adapter route preview', () => {
  const resolver = {
    getAccountById: getMockAccountById,
    getProviderById: getMockProviderById,
    upsertGeneratedProvider: upsertMockProvider,
    removeGeneratedProvider: removeMockProvider,
  };

  beforeEach(() => {
    resetMockAdapters();
    resetMockProviders();
  });

  it('classifies a randomly named saved provider and keeps secrets out of analysis and plan', async () => {
    const sourceId = `kimi-provider-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'kimi',
      name: 'Kimi membership',
      preset: 'kimi-code-membership',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    resetMockAdapters();
    const adapter = createMockAdapterPort(resolver);

    const native = await adapter.analyze({ sourceKind: 'provider', sourceId, targetAgentId: 'claude' });
    const local = await adapter.analyze({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });
    const sync = await adapter.plan({ sourceKind: 'provider', sourceId, targetAgentId: 'pi' });
    expect(native.route).toBe('native_endpoint');
    expect(local.route).toBe('local_bridge');
    expect(sync.analysis.route).toBe('config_sync');
    expect(sync.changes).toEqual([
      { target: 'pi', field: 'provider', value: 'kimi-for-coding', secret: false },
      { target: 'pi', field: 'apiKey', secret: true },
    ]);
    expect(JSON.stringify({ native, sync })).not.toContain('must-not-leak');
    expect(native.actions.find((item) => item.secret)).not.toHaveProperty('value');

    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'claude' });
    const repeated = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'claude' });
    expect(applied.profile.status).toBe('active');
    expect(repeated.profile.id).toBe(applied.profile.id);
    expect(await adapter.listProfiles()).toHaveLength(1);
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');
  });

  it('applies a local bridge, exposes its generated Codex Connection, and controls status without tokens', async () => {
    resetMockAdapters();
    const sourceId = `kimi-bridge-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'kimi',
      name: 'Kimi membership',
      preset: 'kimi-code-membership',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const plan = await adapter.plan({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });
    expect(plan.canApply).toBe(true);
    expect(plan.serviceImpact).toBe('requires_local_bridge');

    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });
    const visible = await createMockProviderPort().listProviders('codex');
    expect(visible.find((provider) => provider.id === applied.provider.id)).toMatchObject({ isCurrent: true });
    expect(applied.profile.autoStart).toBe(true);
    expect(await adapter.getBridgeStatus(applied.profile.id)).toMatchObject({ state: 'running', port: 32123 });
    expect(await adapter.stopBridge(applied.profile.id)).toMatchObject({ state: 'stopped' });
    expect(await adapter.startBridge(applied.profile.id)).toMatchObject({ state: 'running' });
    expect((await adapter.setBridgeAutoStart(applied.profile.id, false)).autoStart).toBe(false);
    expect(JSON.stringify({ plan, applied, visible })).not.toContain('must-not-leak');
    expect(JSON.stringify({ plan, applied, visible })).not.toContain('token');
  });

  it('refuses to remove the active generated Connection, then removes an inactive projection and its provider', async () => {
    const sourceId = `kimi-remove-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'kimi',
      name: 'Kimi membership',
      preset: 'kimi-code-membership',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });

    await expect(adapter.remove(applied.profile.id)).rejects.toThrow('切换到其他连接');
    expect(await adapter.listProfiles()).toHaveLength(1);
    expect(getMockProviderById(applied.provider.id)).toMatchObject({ isCurrent: true });

    upsertMockProvider({ ...applied.provider, isCurrent: false });
    await adapter.remove(applied.profile.id);

    expect(await adapter.listProfiles()).toEqual([]);
    expect(getMockProviderById(applied.provider.id)).toBeUndefined();
  });

  it('resets profiles, bridge state, and generated providers without leaking into a fresh factory', async () => {
    const sourceId = `kimi-reset-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'kimi',
      name: 'Kimi membership',
      preset: 'kimi-code-membership',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });
    expect(await adapter.getBridgeStatus(applied.profile.id)).toMatchObject({ state: 'running' });

    resetMockAdapters();

    expect(await adapter.listProfiles()).toEqual([]);
    expect(getMockProviderById(applied.provider.id)).toBeUndefined();
    const freshAdapter = createMockAdapterPort(resolver);
    expect(await freshAdapter.listProfiles()).toEqual([]);
  });
});
