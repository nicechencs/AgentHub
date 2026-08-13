import { beforeEach, describe, expect, it } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { Account, AgentId, Provider } from '@/lib/types';
import { createMockAdapterPort, resetMockAdapters } from './adapter';
import { getMockAccountById } from './account';
import contract from './fixtures/adapter-capability-contract.json';
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
    // Desktop apply forces opt-in auto-start; mock must not invent true.
    expect(applied.profile.autoStart).toBe(false);
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

  it('keeps Codex OAuth without auth_json as a closed Claude subscription surface', async () => {
    const accountId = 'codex-oauth-account';
    const adapter = createMockAdapterPort({
      getAccountById: (id) => (id === accountId
        ? {
            id: accountId,
            agentId: 'codex',
            kind: 'oauth',
            label: 'ChatGPT subscription',
            isCurrent: true,
            tokenValid: true,
          }
        : getMockAccountById(id)),
      getProviderById: getMockProviderById,
    });

    const analysis = await adapter.analyze({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    });
    const plan = await adapter.plan({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    });

    expect(analysis.route).toBe('unsupported');
    expect(analysis.support).toBe('unsupported');
    expect(analysis.gateKind).toBe('subscription_candidate');
    expect(analysis.ruleId).toBeNull();
    expect(analysis.reason).toContain('当前不支持');
    expect(analysis.reason).toContain('门禁');
    expect(analysis.reason).toMatch(/Claude/);
    expect(analysis.reason).toMatch(/API Key|官方登录/);
    expect(plan.canApply).toBe(false);
    expect(plan.changes).toEqual([]);
    await expect(adapter.apply({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    })).rejects.toThrow(/不可应用|不支持|canApply/i);
    expect(JSON.stringify({ analysis, plan })).not.toMatch(/sk-|access_token|refresh_token|bearer/i);
  });

  it('throws AdapterCommandError with a structured not-retryable shape', async () => {
    const adapter = createMockAdapterPort(resolver);
    await expect(adapter.analyze({
      sourceKind: 'provider',
      sourceId: 'missing-source',
      targetAgentId: 'claude',
    })).rejects.toMatchObject({
      name: 'AdapterCommandError',
      code: 'not_found',
      message: 'provider not found: missing-source',
      retryable: false,
    });

    await expect(adapter.apply({
      sourceKind: 'account',
      sourceId: 'missing-account',
      targetAgentId: 'claude',
    })).rejects.toBeInstanceOf(AdapterCommandError);

    await expect(adapter.remove('missing-profile')).rejects.toMatchObject({
      name: 'AdapterCommandError',
      code: 'not_found',
      retryable: false,
    });
    await expect(adapter.startBridge('missing-profile')).rejects.toMatchObject({
      code: 'not_found',
      retryable: false,
    });
  });

  it('persists the same local-bridge ruleId used by production AdapterBridgeService', async () => {
    const sourceId = `kimi-ruleid-${Date.now()}-${Math.random()}`;
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
    expect(applied.profile.ruleId).toBe('kimi-membership-to-codex-v1');
  });
});

type ContractCase = (typeof contract.cases)[number];

function contractAccount(id: string, source: ContractCase['source']): Account {
  return {
    id,
    agentId: source.agentId as AgentId,
    kind: (source.accountKind ?? 'oauth') as Account['kind'],
    label: id,
    isCurrent: false,
    tokenValid: true,
    credentialFormat: 'credentialFormat' in source ? source.credentialFormat : undefined,
    credentials: 'credentials' in source ? source.credentials : undefined,
  } as Account & { credentials?: Record<string, unknown> };
}

describe('shared adapter capability contract', () => {
  beforeEach(() => {
    resetMockAdapters();
    resetMockProviders();
  });

  it.each(contract.cases)('$id matches Rust analyze/plan surface', async (item) => {
    const sourceId = `contract-${item.id}`;
    const accounts = new Map<string, Account>();
    if (item.source.kind === 'provider') {
      upsertMockProvider({
        id: sourceId,
        agentId: item.source.agentId as AgentId,
        name: item.id,
        preset: item.source.preset ?? 'default',
        configText: '{}',
        configFormat: 'json',
        isCurrent: false,
      } satisfies Provider);
    } else {
      accounts.set(sourceId, contractAccount(sourceId, item.source));
    }
    const adapter = createMockAdapterPort({
      getAccountById: (id) => accounts.get(id) ?? getMockAccountById(id),
      getProviderById: getMockProviderById,
    });
    const request = {
      sourceKind: item.source.kind as 'account' | 'provider',
      sourceId,
      targetAgentId: item.target as AgentId,
    };
    const analysis = await adapter.analyze(request);
    const plan = await adapter.plan(request);
    expect(analysis.route).toBe(item.expect.route);
    expect(analysis.support).toBe(item.expect.support);
    expect(analysis.ruleId ?? null).toBe(item.expect.ruleId);
    expect(analysis.gateKind).toBe(item.expect.gateKind);
    expect(analysis.reason).toBe(item.expect.reason);
    expect(plan.canApply).toBe(item.expect.canApply);
  });
});
