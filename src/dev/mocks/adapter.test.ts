import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { Account } from '@/lib/types';
import {
  createMockAdapterPort,
  resetGoldenLookupStats,
  resetMockAdapters,
  seedMockAdapterProfiles,
  setMockRoutePoolV2,
} from './adapter';
import type { MockAdapterApplyPlan } from './adapter/plan';
import { getMockAccountById } from './account';
import {
  CONNECT_FLOW_FIXTURE_IDS,
  seedConnectFlowAdapterFixtures,
} from './connect-flow-fixtures';
import {
  createMockProviderPort,
  getMockProviderById,
  removeMockProvider,
  resetMockProviders,
  upsertMockProvider,
} from './provider';

describe('mock adapter projection', () => {
  const resolver = {
    getAccountById: getMockAccountById,
    getProviderById: getMockProviderById,
    upsertGeneratedProvider: upsertMockProvider,
    removeGeneratedProvider: removeMockProvider,
  };

  beforeEach(() => {
    resetMockAdapters();
    resetMockProviders();
    resetGoldenLookupStats();
  });

  it('does not import classify* on the runtime source-ticket path', () => {
    const src = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), 'adapter/source-ticket.ts'),
      'utf8',
    );
    expect(src).not.toMatch(/\bclassify(Account|Provider)Source\b/);
  });

  it('plan carries sourceProduct from the plan owner', async () => {
    const sourceId = `kimi-product-${Date.now()}-${Math.random()}`;
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
    const plan = await adapter.plan({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'claude',
    }) as MockAdapterApplyPlan;
    expect(plan.sourceProduct).toBe('kimi-code-membership');
    expect(plan.analysis.route).toBe('native_endpoint');
  });

  it('keeps secrets out of analyze, plan, and apply', async () => {
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
    const sync = await adapter.plan({ sourceKind: 'provider', sourceId, targetAgentId: 'pi' });
    expect(native.actions.find((item) => item.secret)).not.toHaveProperty('value');
    expect(JSON.stringify({ native, sync })).not.toContain('must-not-leak');

    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'claude' });
    const repeated = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'claude' });
    expect(applied.profile.status).toBe('active');
    expect(repeated.profile.id).toBe(applied.profile.id);
    expect(await adapter.listProfiles()).toHaveLength(1);
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');
  });

  it('applies a local bridge, exposes its generated Codex Connection, and controls status without tokens', async () => {
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
    expect(plan.serviceImpact).toBe('requires_local_bridge');

    const applied = await adapter.apply({ sourceKind: 'provider', sourceId, targetAgentId: 'codex' });
    const visible = await createMockProviderPort().listProviders('codex');
    expect(visible.find((provider) => provider.id === applied.provider.id)).toMatchObject({ isCurrent: true });
    expect(applied.profile.autoStart).toBe(false);
    expect(applied.profile.ruleId).toBe(plan.analysis.ruleId);
    expect(await adapter.getBridgeStatus(applied.profile.id)).toMatchObject({ state: 'running', port: 32123 });
    expect(await adapter.stopBridge(applied.profile.id)).toMatchObject({ state: 'stopped' });
    expect(await adapter.startBridge(applied.profile.id)).toMatchObject({ state: 'running' });
    expect((await adapter.setBridgeAutoStart(applied.profile.id, false)).autoStart).toBe(false);
    expect(JSON.stringify({ plan, applied, visible })).not.toContain('must-not-leak');
    expect(JSON.stringify({ plan, applied, visible })).not.toContain('token');
  });

  it('enrolls a native login into the default local pool only when the flag is on', async () => {
    const sourceId = `kimi-enroll-${Date.now()}-${Math.random()}`;
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
    const nativeId = `native-${sourceId}`;
    seedMockAdapterProfiles([{
      id: nativeId,
      name: 'Kimi',
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'codex',
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: 'kimi-membership-to-codex-v1',
      ruleVersion: '1',
      generatedProviderId: null,
      localPort: null,
      autoStart: false,
      createdAt: '2026-08-12T00:00:00Z',
      updatedAt: '2026-08-12T00:00:00Z',
    }]);
    await expect(adapter.listDefaultRoutePools()).resolves.toMatchObject({ enabled: true });
    setMockRoutePoolV2(false);
    await expect(adapter.listDefaultRoutePools()).resolves.toEqual({ enabled: false, pools: [] });
    await expect(adapter.enrollNativeToGateway(nativeId)).rejects.toMatchObject({
      code: 'unsupported',
    });
    setMockRoutePoolV2(true);
    const enrolled = await adapter.enrollNativeToGateway(nativeId);
    expect(enrolled.v2Enrolled).toBe(true);
    expect(enrolled.gatewayPort).toBeGreaterThan(0);
    expect(JSON.stringify(enrolled)).not.toContain('must-not-leak');
    expect(JSON.stringify(enrolled)).not.toContain('hubToken');
    expect(JSON.stringify(enrolled)).not.toContain('ahb_');
  });

  it('attaches a provider to the default auth pool without listing it as a connection ticket', async () => {
    const sourceId = `codex-pool-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'codex',
      name: 'Codex API',
      preset: 'custom',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const overview = await adapter.attachPoolOwnedAuthorization({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'codex',
      surface: 'responses',
    });
    expect(overview.targetAgentId).toBe('codex');
    expect(overview.surface).toBe('responses');
    expect(overview.members).toEqual([
      expect.objectContaining({ sourceKind: 'provider', sourceId, enabled: true }),
    ]);
    expect(getMockProviderById(sourceId)?.home).toBe('route_pool');
    expect(JSON.stringify(overview)).not.toContain('must-not-leak');

    const removed = await adapter.removeRouteAuthorization('provider', sourceId);
    expect(removed).toBe(1);
    const listed = await adapter.listDefaultRoutePools();
    expect(listed.pools.every((pool) => (
      pool.members.every((member) => member.sourceId !== sourceId)
    ))).toBe(true);
    expect(getMockProviderById(sourceId)).toBeTruthy();
  });

  it('syncs Connections authorizations into the default pool without hiding them', async () => {
    const sourceId = `codex-conn-${Date.now()}-${Math.random()}`;
    await createMockProviderPort().upsertProvider({
      id: sourceId,
      agentId: 'codex',
      name: 'Connection API',
      preset: 'custom',
      configText: 'api_key = "must-not-leak"',
      configFormat: 'toml',
      isCurrent: false,
    });
    const adapter = createMockAdapterPort(resolver);
    const first = await adapter.syncConnectionAuthorizations();
    expect(first.added).toBeGreaterThan(0);
    const listed = await adapter.listDefaultRoutePools();
    expect(listed.pools.some((pool) => (
      pool.members.some((member) => member.sourceId === sourceId)
    ))).toBe(true);
    expect(getMockProviderById(sourceId)?.home).not.toBe('route_pool');
    const second = await adapter.syncConnectionAuthorizations();
    expect(second.added).toBe(0);
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

  it('rejects apply when the plan is closed and does not leak credentials', async () => {
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
    const plan = await adapter.plan({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    });
    expect(plan.canApply).toBe(false);
    await expect(adapter.apply({
      sourceKind: 'account',
      sourceId: accountId,
      targetAgentId: 'claude',
    })).rejects.toThrow(/不可应用|不支持|canApply/i);
    expect(await adapter.listProfiles()).toEqual([]);
    expect(JSON.stringify(plan)).not.toMatch(/sk-|access_token|refresh_token|bearer/i);
  });

  it('applies an oauth local-bridge plan into memory without leaking tokens', async () => {
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
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    });
    const request = {
      sourceKind: 'account' as const,
      sourceId: accountId,
      targetAgentId: 'claude' as const,
    };
    const plan = await adapter.plan(request);
    const applied = await adapter.apply(request);
    expect(applied.profile.route).toBe(plan.analysis.route);
    expect(applied.profile.ruleId).toBe(plan.analysis.ruleId);
    expect(applied.profile.mode).toBe('oauth');
    expect(JSON.parse(applied.provider.configText)).toEqual({
      env: {
        ANTHROPIC_BASE_URL: 'http://127.0.0.1:32123',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    });
    expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
  });

  it('applies oauth reuse into Pi as memory profiles without leaking tokens', async () => {
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
    ]);
    const adapter = createMockAdapterPort({
      getAccountById: (id) => accounts.get(id),
      getProviderById: getMockProviderById,
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    });
    for (const sourceId of ['claude-subscription', 'codex-auth-json'] as const) {
      const applied = await adapter.apply({
        sourceKind: 'account',
        sourceId,
        targetAgentId: 'pi',
      });
      expect(applied.profile.mode).toBe('oauth');
      expect(applied.provider.agentId).toBe('pi');
      expect(applied.provider.configText).not.toContain('must-not-leak');
    }
    expect((await adapter.listProfiles()).length).toBe(2);
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

  it('applies Kimi membership → Pi via config_sync without leaking secrets', async () => {
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const adapter = createMockAdapterPort(resolver);
    const request = {
      sourceKind: 'provider' as const,
      sourceId: kimiMembership.id,
      targetAgentId: 'pi' as const,
    };
    const plan = await adapter.plan(request);
    const applied = await adapter.apply(request);
    const repeated = await adapter.apply(request);
    expect(applied.profile.id).toBe(`adapter-kimi-pi-${CONNECT_FLOW_FIXTURE_IDS.kimiMembership}`);
    expect(applied.profile.route).toBe(plan.analysis.route);
    expect(applied.profile.ruleId).toBe(plan.analysis.ruleId);
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
    const applied = await adapter.apply(request);
    const repeated = await adapter.apply(request);
    expect(applied.profile.id).toBe(`adapter-anthropic-pi-${CONNECT_FLOW_FIXTURE_IDS.anthropic}`);
    expect(applied.profile.ruleId).toBe(plan.analysis.ruleId);
    expect(repeated.profile.id).toBe(applied.profile.id);
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
    expect(JSON.stringify({ appliedClaude, appliedPi })).not.toContain('must-not-leak');
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
      preset: 'custom',
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

  it('applies GLM / DeepSeek → Claude with projected URLs without leaking secrets', async () => {
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

    const glmApplied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: 'glm-src',
      targetAgentId: 'claude',
    });
    expect(JSON.parse(glmApplied.provider.configText)).toEqual({
      env: {
        ANTHROPIC_BASE_URL: 'https://open.bigmodel.cn/api/anthropic',
        ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
      },
    });

    const deepseekApplied = await adapter.apply({
      sourceKind: 'account',
      sourceId: 'deepseek-acc',
      targetAgentId: 'claude',
    });
    expect(JSON.parse(deepseekApplied.provider.configText).env.ANTHROPIC_BASE_URL)
      .toBe('https://api.deepseek.com/anthropic');
    expect(JSON.stringify({ glmApplied, deepseekApplied })).not.toContain('must-not-leak');
  });

  it('applies GLM / DeepSeek → Codex Responses config without leaking secrets', async () => {
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
        baseUrl: 'https://open.bigmodel.cn/api/v1',
        slug: 'agenthub_glm',
        model: 'glm-5.3',
      },
      {
        sourceKind: 'account' as const,
        sourceId: deepseekAccount.id,
        baseUrl: 'https://api.deepseek.com',
        slug: 'agenthub_deepseek',
        model: 'deepseek-v4-flash',
      },
    ] as const;

    for (const item of cases) {
      const request = { sourceKind: item.sourceKind, sourceId: item.sourceId, targetAgentId: 'codex' as const };
      const plan = await adapter.plan(request);
      const applied = await adapter.apply(request);
      expect(applied.profile.route).toBe(plan.analysis.route);
      expect(applied.profile.ruleId).toBe(plan.analysis.ruleId);
      expect(applied.provider.configText).toContain(`model_provider = "${item.slug}"`);
      expect(applied.provider.configText).toContain(`model = "${item.model}"`);
      expect(applied.provider.configText).toContain(`base_url = "${item.baseUrl}"`);
      expect(applied.provider.configText).toContain('wire_api = "responses"');
      expect(applied.provider.configText).toContain('$AGENTHUB_CONNECTION_SECRET$');
      expect(JSON.stringify({ plan, applied })).not.toContain('must-not-leak');
    }
  });

  it('applies GLM / DeepSeek → Pi as custom providers without leaking secrets', async () => {
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
    expect(JSON.parse(glm.provider.configText).models.providers['glm-coding-plan']).toEqual({
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
    expect(JSON.parse(deepseek.provider.configText).models.providers.deepseek).toEqual({
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

    const applied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'dsh',
    });
    expect(applied.provider.configText).toContain('$AGENTHUB_CONNECTION_SECRET$');
    expect(JSON.stringify(applied)).not.toContain('must-not-leak');

    const hostApplied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: hostId,
      targetAgentId: 'dsh',
    });
    expect(hostApplied.provider.configText).toContain('$AGENTHUB_CONNECTION_SECRET$');

    const claudeApplied = await adapter.apply({
      sourceKind: 'provider',
      sourceId: presetId,
      targetAgentId: 'claude',
    });
    expect(JSON.parse(claudeApplied.provider.configText).env.ANTHROPIC_BASE_URL)
      .toBe('https://api.deepseek.com/anthropic');
    expect(JSON.stringify(claudeApplied)).not.toContain('must-not-leak');
  });

  it('keeps an unknown dsh-only source fail-closed', async () => {
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
    expect(JSON.stringify(plan)).not.toContain('must-not-leak');
    await expect(adapter.apply({
      sourceKind: 'provider',
      sourceId,
      targetAgentId: 'dsh',
    })).rejects.toThrow(/不可应用|不支持|canApply/i);
    expect(await adapter.listProfiles()).toEqual([]);
  });
});
