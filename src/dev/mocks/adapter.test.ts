import { beforeEach, describe, expect, it } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { Account, AgentId, Provider } from '@/lib/types';
import {
  CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
  createMockAdapterPort,
  PROTOCOL_MISMATCH_REASON,
  resetMockAdapters,
} from './adapter';
import { getMockAccountById, upsertMockAccount } from './account';
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

  it('classifies and applies a Kimi membership Account on all three provider edges', async () => {
    const sourceId = `kimi-account-${Date.now()}-${Math.random()}`;
    upsertMockAccount({
      id: sourceId,
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi Code membership',
      isCurrent: false,
      tokenValid: true,
      credentials: {
        format: 'api_key',
        api_key: 'must-not-leak',
        provider: 'kimi-code-membership',
      },
      extra: {},
    } as Account & {
      credentials: Record<string, unknown>;
      extra: Record<string, unknown>;
    });
    const adapter = createMockAdapterPort(resolver);

    for (const targetAgentId of ['claude', 'pi', 'codex'] as const) {
      const plan = await adapter.plan({
        sourceKind: 'account',
        sourceId,
        targetAgentId,
      });
      expect(plan.canApply).toBe(true);
      const applied = await adapter.apply({
        sourceKind: 'account',
        sourceId,
        targetAgentId,
      });
      expect(applied.profile.sourceKind).toBe('account');
      expect(applied.profile.ruleId).toBe(
        targetAgentId === 'claude'
          ? 'kimi-membership-to-claude-v1'
          : targetAgentId === 'pi'
            ? 'kimi-membership-to-pi-v1'
            : 'kimi-membership-to-codex-v1',
      );
      expect(JSON.stringify(applied)).not.toContain('must-not-leak');
    }

    const bareId = `${sourceId}-bare`;
    upsertMockAccount({
      id: bareId,
      agentId: 'kimi',
      kind: 'apikey',
      label: 'Kimi API',
      isCurrent: false,
      tokenValid: true,
    });
    const oauthId = `${sourceId}-oauth`;
    upsertMockAccount({
      id: oauthId,
      agentId: 'kimi',
      kind: 'oauth',
      label: 'Kimi managed OAuth',
      isCurrent: false,
      tokenValid: true,
    });
    expect((await adapter.plan({
      sourceKind: 'account',
      sourceId: bareId,
      targetAgentId: 'claude',
    })).canApply).toBe(false);
    expect((await adapter.plan({
      sourceKind: 'account',
      sourceId: oauthId,
      targetAgentId: 'pi',
    })).canApply).toBe(false);
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
    expect(plan.reusePath).toBe('none');
    expect(plan.changes).toEqual([]);
    await expect(adapter.apply({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    })).rejects.toThrow(/不可应用|不支持|canApply/i);
    expect(JSON.stringify({ analysis, plan })).not.toMatch(/sk-|access_token|refresh_token|bearer/i);
    expect(plan.reason).not.toContain('同边但暂不可写');
  });

  it('opens Codex auth_json into the experimental Claude local bridge', async () => {
    const accountId = 'codex-auth-json-claude';
    const account = {
      id: accountId,
      agentId: 'codex' as const,
      kind: 'oauth' as const,
      label: 'ChatGPT subscription',
      isCurrent: true,
      tokenValid: true,
      credentialFormat: 'auth_json',
      credentials: {
        format: 'auth_json',
        tokens: {
          access_token: 'must-not-leak',
          refresh_token: 'must-not-leak',
        },
      },
    };
    const adapter = createMockAdapterPort({
      getAccountById: (id) => id === accountId ? account : getMockAccountById(id),
      getProviderById: getMockProviderById,
    });
    const request = {
      sourceKind: 'account' as const,
      sourceId: accountId,
      targetAgentId: 'claude' as const,
    };

    const plan = await adapter.plan(request);
    expect(plan).toMatchObject({
      canApply: true,
      reusePath: 'local_bridge',
      serviceImpact: 'requires_local_bridge',
      analysis: {
        route: 'local_bridge',
        support: 'experimental',
        ruleId: 'codex-subscription-to-claude-responses-v1',
        gateKind: 'none',
      },
    });
    expect(plan.changes).toEqual([
      {
        target: 'claude',
        field: 'ANTHROPIC_BASE_URL',
        value: 'http://127.0.0.1:<本机端口>',
        secret: false,
      },
      { target: 'claude', field: 'ANTHROPIC_AUTH_TOKEN', secret: true },
    ]);
    expect(plan.reason).toBe(CODEX_SUBSCRIPTION_TO_CLAUDE_REASON);

    const applied = await adapter.apply(request);
    expect(applied.profile).toMatchObject({
      targetAgentId: 'claude',
      route: 'local_bridge',
      mode: 'oauth',
      ruleId: 'codex-subscription-to-claude-responses-v1',
    });
    expect(JSON.parse(applied.provider.configText)).toEqual({
      env: {
        ANTHROPIC_BASE_URL: 'http://127.0.0.1:32123',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    });
    expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
  });

  it('opens Claude, Codex, and Grok OAuth reuse into Pi for experimental bind', async () => {
    const accounts = new Map<string, Account>([
      ['claude-subscription', {
        id: 'claude-subscription',
        agentId: 'claude',
        kind: 'oauth',
        label: 'Claude subscription',
        isCurrent: false,
        tokenValid: true,
      }],
      ['codex-auth-json', {
        id: 'codex-auth-json',
        agentId: 'codex',
        kind: 'oauth',
        label: 'Codex auth.json',
        isCurrent: false,
        tokenValid: true,
        credentialFormat: 'auth_json',
      }],
      ['codex-oauth-other', {
        id: 'codex-oauth-other',
        agentId: 'codex',
        kind: 'oauth',
        label: 'Codex OAuth',
        isCurrent: false,
        tokenValid: true,
      }],
      ['grok-subscription', {
        id: 'grok-subscription',
        agentId: 'grok',
        kind: 'oauth',
        label: 'Grok subscription',
        isCurrent: false,
        tokenValid: true,
      }],
    ]);
    const adapter = createMockAdapterPort({
      getAccountById: (id) => accounts.get(id),
      getProviderById: getMockProviderById,
    });
    const cases = [
      {
        sourceId: 'claude-subscription',
        value: 'anthropic',
        ruleId: 'claude-subscription-to-pi-v1',
        reason: 'Claude 订阅可写入 Pi 的 anthropic 登录槽（原生订阅复用）。',
      },
      {
        sourceId: 'codex-auth-json',
        value: 'openai-codex',
        ruleId: 'codex-subscription-to-pi-v1',
        reason: 'Codex / ChatGPT 订阅可写入 Pi 的 openai-codex 登录槽（原生订阅复用）。',
      },
      {
        sourceId: 'codex-oauth-other',
        value: 'openai-codex',
        ruleId: 'codex-subscription-to-pi-v1',
        reason: 'Codex / ChatGPT 订阅可写入 Pi 的 openai-codex 登录槽（原生订阅复用）。',
      },
      {
        sourceId: 'grok-subscription',
        value: 'xai',
        ruleId: 'grok-subscription-to-pi-v1',
        reason: 'Grok / xAI 订阅可写入 Pi 的 xai 登录槽（原生订阅复用）。',
      },
    ] as const;

    for (const item of cases) {
      const plan = await adapter.plan({
        sourceKind: 'account',
        sourceId: item.sourceId,
        targetAgentId: 'pi',
      });
      expect(plan).toMatchObject({
        analysis: {
          route: 'config_sync',
          support: 'experimental',
          gateKind: 'none',
          ruleId: item.ruleId,
          reason: item.reason,
          actions: [
            {
              kind: 'set_config',
              target: 'Pi',
              value: item.value,
              secret: false,
            },
            {
              kind: 'reference_connection_secret',
              target: 'Pi',
              description: '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。',
              secret: true,
            },
          ],
          limitations: [
            '会把 OAuth access/refresh 写入 Pi auth.json 对应槽；预览、IPC、日志不传输明文 token。',
            '写入后由 Pi 刷新该槽；Hub 不双刷同一 refresh token。原 Agent 与 Pi 同时刷新可能互相打翻。',
            '实验性：应用后会把生成 Provider 设为 Pi 当前连接。',
          ],
        },
        canApply: true,
        maturity: 'experimental',
        reusePath: 'native_subscription',
        serviceImpact: 'none',
        changes: [
          { target: 'pi', field: 'provider', value: item.value, secret: false },
          { target: 'pi', field: 'auth', secret: true },
        ],
      });
      const applied = await adapter.apply({
        sourceKind: 'account',
        sourceId: item.sourceId,
        targetAgentId: 'pi',
      });
      expect(applied.profile.mode).toBe('oauth');
      expect(applied.provider.configText).not.toContain('must-not-leak');
    }
    expect((await adapter.listProfiles()).length).toBe(4);
  });

  it('keeps subscription protocol mismatches unsupported', async () => {
    const accounts = new Map<string, Account>([
      ['claude-subscription', {
        id: 'claude-subscription',
        agentId: 'claude',
        kind: 'oauth',
        label: 'Claude subscription',
        isCurrent: false,
        tokenValid: true,
      }],
      ['grok-subscription', {
        id: 'grok-subscription',
        agentId: 'grok',
        kind: 'oauth',
        label: 'Grok subscription',
        isCurrent: false,
        tokenValid: true,
      }],
    ]);
    const adapter = createMockAdapterPort({
      getAccountById: (id) => accounts.get(id),
      getProviderById: getMockProviderById,
    });
    const claudeToCodex = await adapter.plan({
      sourceKind: 'account',
      sourceId: 'claude-subscription',
      targetAgentId: 'codex',
    });
    const grokToClaude = await adapter.plan({
      sourceKind: 'account',
      sourceId: 'grok-subscription',
      targetAgentId: 'claude',
    });
    expect(claudeToCodex).toMatchObject({
      analysis: { route: 'unsupported', reason: PROTOCOL_MISMATCH_REASON },
      canApply: false,
      reusePath: 'none',
    });
    expect(grokToClaude).toMatchObject({
      analysis: { route: 'unsupported', reason: PROTOCOL_MISMATCH_REASON },
      canApply: false,
      reusePath: 'none',
    });
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

  it('plans and applies GLM / DeepSeek → Codex through official Responses endpoints', async () => {
    await createMockProviderPort().upsertProvider({
      id: 'glm-codex-src',
      agentId: 'claude',
      name: 'GLM Codex',
      preset: 'glm-coding-plan',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const deepseekAccount = {
      id: 'deepseek-codex-acc',
      agentId: 'claude' as const,
      kind: 'apikey' as const,
      label: 'DeepSeek Codex',
      isCurrent: false,
      tokenValid: true,
      extra: { provider: 'deepseek-api' },
      credentials: { format: 'api_key', api_key: 'must-not-leak' },
    } as Account;
    const adapter = createMockAdapterPort({
      getAccountById: (id) => id === deepseekAccount.id ? deepseekAccount : getMockAccountById(id),
      getProviderById: getMockProviderById,
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    });

    const cases = [
      {
        sourceKind: 'provider' as const,
        sourceId: 'glm-codex-src',
        ruleId: 'glm-coding-plan-to-codex-v1',
        baseUrl: 'https://open.bigmodel.cn/api/v1',
        slug: 'agenthub_glm',
        model: 'glm-5.3',
      },
      {
        sourceKind: 'account' as const,
        sourceId: deepseekAccount.id,
        ruleId: 'deepseek-api-to-codex-v1',
        baseUrl: 'https://api.deepseek.com',
        slug: 'agenthub_deepseek',
        model: 'deepseek-v4-flash',
      },
    ] as const;

    for (const item of cases) {
      const request = { sourceKind: item.sourceKind, sourceId: item.sourceId, targetAgentId: 'codex' as const };
      const plan = await adapter.plan(request);
      expect(plan).toMatchObject({
        canApply: true,
        reusePath: 'api_endpoint',
        serviceImpact: 'none',
        analysis: {
          route: 'native_endpoint',
          support: 'experimental',
          ruleId: item.ruleId,
          gateKind: 'none',
        },
        changes: [
          { target: 'codex', field: 'provider', secret: false },
          { target: 'codex', field: 'baseUrl', value: item.baseUrl, secret: false },
          { target: 'codex', field: 'wireApi', value: 'responses', secret: false },
        ],
      });
      const applied = await adapter.apply(request);
      expect(applied.profile).toMatchObject({
        route: 'native_endpoint',
        ruleId: item.ruleId,
        targetAgentId: 'codex',
      });
      expect(applied.provider).toMatchObject({
        agentId: 'codex',
        configFormat: 'toml',
      });
      expect(applied.provider.configText).toContain(`model_provider = "${item.slug}"`);
      expect(applied.provider.configText).toContain(`model = "${item.model}"`);
      expect(applied.provider.configText).toContain(`base_url = "${item.baseUrl}"`);
      expect(applied.provider.configText).toContain('wire_api = "responses"');
      expect(applied.provider.configText).toContain('$AGENTHUB_CONNECTION_SECRET$');
      expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
    }
  });

  it('applies GLM / DeepSeek → Pi as custom providers with endpoint contracts', async () => {
    await createMockProviderPort().upsertProvider({
      id: 'glm-pi-src',
      agentId: 'claude',
      name: 'GLM Pi',
      preset: 'glm-coding-plan',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    await createMockProviderPort().upsertProvider({
      id: 'deepseek-pi-src',
      agentId: 'claude',
      name: 'DeepSeek Pi',
      preset: 'deepseek-api',
      configText: '{"apiKey":"must-not-leak"}',
      configFormat: 'json',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort({
      getAccountById: getMockAccountById,
      getProviderById: getMockProviderById,
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    });

    const glm = await adapter.apply({
      sourceKind: 'provider',
      sourceId: 'glm-pi-src',
      targetAgentId: 'pi',
    });
    const glmConfig = JSON.parse(glm.provider.configText);
    expect(glm.profile.ruleId).toBe('glm-coding-plan-to-pi-v1');
    expect(glmConfig.models.providers['glm-coding-plan']).toEqual({
      baseUrl: 'https://open.bigmodel.cn/api/coding/paas/v4',
      api: 'openai-completions',
      models: [{ id: 'glm-4.6' }],
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });

    const deepseek = await adapter.apply({
      sourceKind: 'provider',
      sourceId: 'deepseek-pi-src',
      targetAgentId: 'pi',
    });
    const deepseekConfig = JSON.parse(deepseek.provider.configText);
    expect(deepseek.profile.ruleId).toBe('deepseek-api-to-pi-v1');
    expect(deepseekConfig.models.providers.deepseek).toEqual({
      baseUrl: 'https://api.deepseek.com',
      api: 'openai-completions',
      models: [{ id: 'deepseek-chat' }],
      apiKey: '$AGENTHUB_CONNECTION_SECRET$',
    });
    expect(JSON.stringify({ glm, deepseek })).not.toContain('must-not-leak');
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

    const claudePlan = await adapter.plan({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'claude',
    });
    expect(claudePlan.canApply).toBe(true);
    expect(claudePlan.analysis.route).toBe('native_endpoint');
    expect(claudePlan.analysis.support).toBe('experimental');
    expect(claudePlan.analysis.ruleId).toBe('deepseek-api-to-claude-v1');

    const claudeApplied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'claude',
    });
    expect(claudeApplied.profile.ruleId).toBe('deepseek-api-to-claude-v1');
    expect(JSON.parse(claudeApplied.provider.configText).env.ANTHROPIC_BASE_URL)
      .toBe('https://api.deepseek.com/anthropic');
    expect(JSON.stringify(claudeApplied)).not.toContain('must-not-leak');
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
    expect(plan.reusePath).toBe(item.expect.reusePath);

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
