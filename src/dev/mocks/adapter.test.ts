import { beforeEach, describe, expect, it } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { Account, AgentId, Provider } from '@/lib/types';
import { createMockAdapterPort, resetMockAdapters } from './adapter';
import { getMockAccountById } from './account';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';
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
    expect(sync.canApply).toBe(true);
    expect(sync.analysis.gateKind).toBe('none');
    expect(sync.analysis.limitations.join('\n')).not.toMatch(/仅预览|Phase 0/);
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

  it('removes the active generated Connection and its provider', async () => {
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
    expect(getMockProviderById(applied.provider.id)).toMatchObject({ isCurrent: true });

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
    expect(plan.reason).not.toContain('同边但暂不可写');
  });

  it('Account Anthropic → Pi is writable on the implemented bind path', async () => {
    const accountId = 'anthropic-account-same-edge';
    const adapter = createMockAdapterPort({
      getAccountById: (id) => (id === accountId
        ? {
            id: accountId,
            agentId: 'claude',
            kind: 'apikey',
            label: 'Anthropic key',
            isCurrent: false,
            tokenValid: true,
            extra: { provider: 'anthropic' },
          } as Account
        : getMockAccountById(id)),
      getProviderById: getMockProviderById,
    });

    const sameEdge = await adapter.plan({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'pi',
    });
    expect(sameEdge.canApply).toBe(true);
    expect(sameEdge.analysis.route).toBe('config_sync');
    expect(sameEdge.reason).toBe(sameEdge.analysis.reason);
    expect(sameEdge.reason).not.toContain('同边但暂不可写');

    const closed = await adapter.plan({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    });
    expect(closed.canApply).toBe(false);
    expect(closed.analysis.route).toBe('unsupported');
    expect(closed.reason).toBe(closed.analysis.reason);
    expect(closed.reason).not.toContain('同边但暂不可写');

    const applied = await adapter.apply({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'pi',
    });
    expect(applied.profile.ruleId).toBe('anthropic-api-to-pi-v1');
    expect(applied.provider.agentId).toBe('pi');
    expect(applied.provider.isCurrent).toBe(true);

    const codex = await adapter.plan({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'codex',
    });
    expect(codex.canApply).toBe(true);
    expect(codex.analysis.route).toBe('local_bridge');
    expect(codex.analysis.ruleId).toBe('anthropic-api-to-codex-v1');
    expect(codex.changes[0].value).toBe('AgentHub Anthropic 本地桥接');
    expect(codex.reason).not.toContain('同边但暂不可写');
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

  it('applies Kimi membership → Pi via config_sync without leaking secrets', async () => {
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const adapter = createMockAdapterPort(resolver);
    const request = {
      sourceKind: 'provider' as const,
      sourceId: kimiMembership.id,
      targetAgentId: 'pi' as const,
    };
    const plan = await adapter.plan(request);
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('config_sync');
    expect(plan.analysis.gateKind).toBe('none');
    expect(plan.analysis.ruleId).toBe('kimi-membership-to-pi-v1');

    const applied = await adapter.apply(request);
    const repeated = await adapter.apply(request);
    expect(applied.profile.id).toBe(`adapter-kimi-pi-${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`);
    expect(applied.profile.route).toBe('config_sync');
    expect(applied.profile.ruleId).toBe('kimi-membership-to-pi-v1');
    expect(repeated.profile.id).toBe(applied.profile.id);
    expect(await adapter.listProfiles()).toHaveLength(1);
    expect(applied.provider).toMatchObject({
      agentId: 'pi',
      isCurrent: true,
    });
    expect(JSON.parse(applied.provider.configText)).toEqual({
      slot: 'kimi-for-coding',
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });
    expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
    expect(applied.provider.configText).not.toMatch(/sk-/i);
  });

  it('applies Anthropic API → Pi via config_sync without leaking secrets', async () => {
    const { anthropic } = seedConnectFlowAdapterFixtures();
    expect(anthropic).toBeDefined();
    const adapter = createMockAdapterPort(resolver);
    const request = {
      sourceKind: 'provider' as const,
      sourceId: anthropic!.id,
      targetAgentId: 'pi' as const,
    };
    const plan = await adapter.plan(request);
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('config_sync');
    expect(plan.analysis.ruleId).toBe('anthropic-api-to-pi-v1');

    const applied = await adapter.apply(request);
    const repeated = await adapter.apply(request);
    expect(applied.profile.id).toBe(`adapter-anthropic-pi-${CONNECT_FLOW_FIXTURE_IDS.anthropic}`);
    expect(applied.profile.route).toBe('config_sync');
    expect(applied.profile.ruleId).toBe('anthropic-api-to-pi-v1');
    expect(repeated.profile.id).toBe(applied.profile.id);
    expect(applied.provider).toMatchObject({
      agentId: 'pi',
      isCurrent: true,
    });
    expect(JSON.parse(applied.provider.configText)).toEqual({
      slot: 'anthropic',
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });
    expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
  });

  it('applies coding-endpoint Kimi without preset to Claude and Pi without leaking secrets', async () => {
    const sourceId = `kimi-live-import-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'kimi',
      name: 'Kimi coding live import',
      preset: 'openai-compatible',
      configText: 'base_url = "https://api.kimi.com/coding/v1"\napi_key = "must-not-leak"\n',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const claudePlan = await adapter.plan({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'claude',
    });
    const piPlan = await adapter.plan({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'pi',
    });
    expect(claudePlan.canApply).toBe(true);
    expect(piPlan.canApply).toBe(true);

    const appliedClaude = await adapter.apply({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'claude',
    });
    const appliedPi = await adapter.apply({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'pi',
    });
    expect(appliedClaude.profile.ruleId).toBe('kimi-membership-to-claude-v1');
    expect(appliedPi.profile.ruleId).toBe('kimi-membership-to-pi-v1');
    expect(JSON.parse(appliedClaude.provider.configText)).toEqual({
      env: {
        ANTHROPIC_BASE_URL: 'https://api.kimi.com/coding/',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    });
    expect(JSON.parse(appliedPi.provider.configText)).toEqual({
      slot: 'kimi-for-coding',
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });
    expect(JSON.stringify({ claudePlan, piPlan, appliedClaude, appliedPi })).not.toContain('must-not-leak');
    expect(getMockProviderById(appliedClaude.provider.id)?.configText).not.toContain('must-not-leak');
    expect(getMockProviderById(appliedPi.provider.id)?.configText).not.toContain('must-not-leak');
  });

  it('rejects moonshot and bare Kimi apply without creating a profile', async () => {
    const moonshotId = `kimi-moonshot-${Date.now()}`;
    const bareId = `kimi-bare-${Date.now()}`;
    await createMockProviderPort().upsertProvider({
      id: moonshotId,
      agentId: 'kimi',
      name: 'Moonshot',
      preset: 'moonshot',
      configText: 'base_url = "https://api.moonshot.cn/v1"\napi_key = "must-not-leak"\n',
      configFormat: 'toml',
      isCurrent: false,
    });
    await createMockProviderPort().upsertProvider({
      id: bareId,
      agentId: 'kimi',
      name: 'Bare Kimi',
      preset: 'openai-compatible',
      configText: 'api_key = "must-not-leak"\n',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);

    for (const sourceId of [moonshotId, bareId]) {
      for (const targetAgentId of ['claude', 'pi'] as const) {
        await expect(adapter.apply({
          sourceKind: 'provider',
          sourceId,
          targetAgentId,
        })).rejects.toThrow(/不可应用|不支持|invalid adapter secret reference/i);
      }
    }
    expect(await adapter.listProfiles()).toEqual([]);
    expect(getMockProviderById(moonshotId)?.configText).toContain('must-not-leak');
    expect((await createMockProviderPort().listProviders('claude'))).toEqual([]);
    expect((await createMockProviderPort().listProviders('pi'))).toEqual([]);
  });

  it('plans and applies GLM / DeepSeek → Claude with rule-specific URLs', async () => {
    await createMockProviderPort().upsertProvider({
      id: 'glm-src',
      agentId: 'claude',
      name: 'GLM',
      preset: 'glm-coding-plan',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort({
      getAccountById: (id) => (id === 'deepseek-acc'
        ? {
            id: 'deepseek-acc',
            agentId: 'claude',
            kind: 'apikey',
            label: 'DeepSeek',
            isCurrent: false,
            tokenValid: true,
            extra: { provider: 'deepseek-api' },
            credentials: { format: 'api_key', api_key: 'must-not-leak' },
          } as Account
        : getMockAccountById(id)),
      getProviderById: getMockProviderById,
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    });

    const glmPlan = await adapter.plan({
      sourceKind: 'provider',
      sourceId: 'glm-src',
      targetAgentId: 'claude',
    });
    expect(glmPlan.canApply).toBe(true);
    expect(glmPlan.analysis.ruleId).toBe('glm-coding-plan-to-claude-v1');
    expect(glmPlan.changes[0].value).toBe('https://open.bigmodel.cn/api/anthropic');
    const glmApplied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: 'glm-src',
      targetAgentId: 'claude',
    });
    expect(glmApplied.profile.ruleId).toBe('glm-coding-plan-to-claude-v1');
    expect(JSON.parse(glmApplied.provider.configText)).toEqual({
      env: {
        ANTHROPIC_BASE_URL: 'https://open.bigmodel.cn/api/anthropic',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    });

    const deepseekPlan = await adapter.plan({
      sourceKind: 'account',
      sourceId: 'deepseek-acc',
      targetAgentId: 'claude',
    });
    expect(deepseekPlan.canApply).toBe(true);
    expect(deepseekPlan.analysis.ruleId).toBe('deepseek-api-to-claude-v1');
    expect(deepseekPlan.changes[0].value).toBe('https://api.deepseek.com/anthropic');
    const deepseekApplied = await adapter.apply({
      sourceKind: 'account',
      sourceId: 'deepseek-acc',
      targetAgentId: 'claude',
    });
    expect(deepseekApplied.profile.ruleId).toBe('deepseek-api-to-claude-v1');
    expect(JSON.parse(deepseekApplied.provider.configText).env.ANTHROPIC_BASE_URL)
      .toBe('https://api.deepseek.com/anthropic');
    expect(JSON.stringify({ glmPlan, deepseekPlan, glmApplied, deepseekApplied }))
      .not.toContain('must-not-leak');
  });

  it('allows removing a current Pi generated Connection', async () => {
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const adapter = createMockAdapterPort(resolver);
    const applied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: kimiMembership.id,
      targetAgentId: 'pi',
    });
    await adapter.remove(applied.profile.id);
    expect(await adapter.listProfiles()).toHaveLength(0);
    expect(getMockProviderById(applied.provider.id)).toBeUndefined();
  });

  it('applies DeepSeek API to dsh by preset or host and keeps the secret out', async () => {
    const presetId = `ds-preset-${Date.now()}`;
    const hostId = `ds-host-${Date.now()}`;
    await createMockProviderPort().upsertProvider({
      id: presetId,
      agentId: 'claude',
      name: 'DeepSeek preset',
      preset: 'deepseek',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'json',
      isCurrent: false,
    });
    await createMockProviderPort().upsertProvider({
      id: hostId,
      agentId: 'kimi',
      name: 'DeepSeek host',
      preset: 'default',
      configText: '{"baseUrl":"https://api.deepseek.com","apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);

    const plan = await adapter.plan({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'dsh',
    });
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('config_sync');
    expect(plan.analysis.ruleId).toBe('deepseek-api-to-dsh-v1');
    expect(plan.changes).toEqual([
      { target: 'dsh', field: 'provider', value: 'deepseek-official', secret: false },
      { target: 'dsh', field: 'apiKeyEnv', value: 'DEEPSEEK_API_KEY', secret: false },
      { target: 'dsh', field: 'apiKey', secret: true },
    ]);
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');

    const applied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'dsh',
    });
    expect(applied.profile.ruleId).toBe('deepseek-api-to-dsh-v1');
    expect(applied.provider.configText).toContain('$AGENTHUB_CONNECTION_SECRET$');
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');

    const hostPlan = await adapter.plan({
      sourceKind: 'provider',
      sourceId: hostId,
      targetAgentId: 'dsh',
    });
    expect(hostPlan.canApply).toBe(true);
    expect(hostPlan.analysis.ruleId).toBe('deepseek-api-to-dsh-v1');

    await expect(
      adapter.apply({
        sourceKind: 'provider',
        sourceId: presetId,
        targetAgentId: 'claude',
      }),
    ).rejects.toThrow(/不可应用|不支持|canApply/i);
  });

  it('does not treat agentId=dsh alone as a DeepSeek API ticket', async () => {
    const sourceId = `dsh-only-${Date.now()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'dsh',
      name: 'DSH row',
      preset: 'default',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const plan = await adapter.plan({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'dsh',
    });
    expect(plan.canApply).toBe(false);
    expect(plan.analysis.route).toBe('unsupported');
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
    extra: 'extra' in source ? source.extra : undefined,
  } as Account & {
    credentials?: Record<string, unknown>;
    extra?: Record<string, unknown>;
  };
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

    // applyPath is the production entry surface (native / local_bridge / config_sync / closed).
    expect(item.expect.applyPath).toBeDefined();
    if (item.expect.applyPath === 'native') {
      expect(item.expect.canApply).toBe(true);
      expect(item.expect.route).toBe('native_endpoint');
      expect(analysis.route).toBe('native_endpoint');
    } else if (item.expect.applyPath === 'local_bridge') {
      expect(item.expect.canApply).toBe(true);
      expect(item.expect.route).toBe('local_bridge');
      expect(analysis.route).toBe('local_bridge');
    } else if (item.expect.applyPath === 'config_sync') {
      expect(item.expect.canApply).toBe(true);
      expect(item.expect.route).toBe('config_sync');
      expect(analysis.route).toBe('config_sync');
      expect(plan.canApply).toBe(true);
    } else {
      expect(item.expect.applyPath).toBe('rejected');
      expect(item.expect.canApply).toBe(false);
      expect(plan.canApply).toBe(false);
      await expect(adapter.apply(request)).rejects.toThrow(/不可应用|不支持|canApply/i);
    }
  });
});
