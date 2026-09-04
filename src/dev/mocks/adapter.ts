/**
 * mock 不是规则真源。规则真源是 core 的 `ADAPTER_CAPABILITY_MATRIX` /
 * `AdapterRouteService` / `AdapterApplyService`。
 */
import {
  adapterCommandError,
  type AdapterApplyRequest,
  type AdapterApplyResult,
  type AdapterBridgeRouteTrace,
  type AdapterBridgeRuntimeStatus,
  type AdapterPort,
  type AdapterProfile,
  type AdapterProfileFilter,
  type DefaultRoutePoolOverview,
  type ForkedConnectionAuthorization,
  type LocalGatewayStatus,
  type RoutePoolDialect,
  type RoutePoolSurface,
  type RouteTracePage,
  type RouteTraceQuery,
} from '@/lib/backend/contracts/adapter';
import type { RouteMembershipTrashPayload } from '@/lib/backend/contracts';
import { delay } from './delay';
import { moveMockMembershipToTrash } from './trash';
import { analyze } from './adapter/analyze';
import { materializeApply } from './adapter/apply';
import { buildPlan } from './adapter/plan';
import type { MockAdapterSourceResolver, MockAdapterState } from './adapter/types';
import { getMockAccountById, listMockAccounts, upsertMockAccount } from './account';
import { getMockProviderById, listMockProviders, upsertMockProvider } from './provider';

const adapterStates = new Set<MockAdapterState>();

const MOCK_POOL_WRITERS = ['claude', 'codex', 'grok', 'kimi', 'dsh'] as const satisfies readonly DefaultRoutePoolOverview['targetAgentId'][];

function mockWriterSurface(agentId: string): RoutePoolSurface | null {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'codex' || agentId === 'grok') return 'responses';
  if (agentId === 'kimi' || agentId === 'dsh') return 'chat_completions';
  return null;
}

function mockWriterDialect(agentId: DefaultRoutePoolOverview['targetAgentId']): RoutePoolDialect {
  if (
    agentId === 'claude'
    || agentId === 'codex'
    || agentId === 'grok'
    || agentId === 'kimi'
    || agentId === 'dsh'
  ) {
    return agentId;
  }
  return 'generic';
}

function mockChatWriter(
  agentId: DefaultRoutePoolOverview['targetAgentId'],
  surface: RoutePoolSurface,
  shared: boolean,
): DefaultRoutePoolOverview['targetAgentId'] {
  if (shared && surface === 'chat_completions' && (agentId === 'kimi' || agentId === 'dsh')) {
    return 'kimi';
  }
  return agentId;
}

function mockChatPool(
  state: MockAdapterState,
  agentId: 'kimi' | 'dsh',
): DefaultRoutePoolOverview {
  const existing = state.defaultPools.find((item) => (
    item.targetAgentId === agentId && item.surface === 'chat_completions'
  ));
  if (existing) return existing;
  const pool: DefaultRoutePoolOverview = {
    id: `pool-${agentId}-chat_completions`,
    targetAgentId: agentId,
    surface: 'chat_completions',
    dialect: agentId,
    unifiedGatewayEnrolled: false,
    members: [],
    listedModels: [],
  };
  state.defaultPools.push(pool);
  return pool;
}

function mockMemberSourceAgent(sourceKind: 'account' | 'provider', sourceId: string): string | undefined {
  return sourceKind === 'account'
    ? getMockAccountById(sourceId)?.agentId
    : getMockProviderById(sourceId)?.agentId;
}

function mockMergeChatPools(state: MockAdapterState): void {
  const dsh = state.defaultPools.find((item) => (
    item.targetAgentId === 'dsh' && item.surface === 'chat_completions'
  ));
  if (!dsh) return;
  const kimi = mockChatPool(state, 'kimi');
  for (const member of dsh.members) {
    if (kimi.members.some((row) => (
      row.sourceKind === member.sourceKind && row.sourceId === member.sourceId
    ))) {
      continue;
    }
    kimi.members.push({ ...member });
  }
  dsh.members = [];
}

function mockSplitChatPools(state: MockAdapterState): void {
  const kimi = state.defaultPools.find((item) => (
    item.targetAgentId === 'kimi' && item.surface === 'chat_completions'
  ));
  if (!kimi) return;
  const dsh = mockChatPool(state, 'dsh');
  const keep: typeof kimi.members = [];
  for (const member of kimi.members) {
    const home = mockMemberSourceAgent(member.sourceKind, member.sourceId);
    const onDsh = dsh.members.some((row) => (
      row.sourceKind === member.sourceKind && row.sourceId === member.sourceId
    ));
    if (home === 'dsh') {
      if (!onDsh) dsh.members.push({ ...member });
      continue;
    }
    keep.push(member);
    if (home !== 'kimi' && !onDsh) dsh.members.push({ ...member });
  }
  kimi.members = keep;
}

function mockPoolTargetsForSync(
  agentId: string,
  kind: 'oauth' | 'apikey',
  shared: boolean,
): Array<{
  agentId: DefaultRoutePoolOverview['targetAgentId'];
  surface: RoutePoolSurface;
  dialect: RoutePoolDialect;
}> {
  if (kind === 'oauth' && agentId !== 'claude' && agentId !== 'codex' && agentId !== 'grok') {
    return [];
  }
  const native = mockWriterSurface(agentId);
  if (native) {
    const target = mockChatWriter(
      agentId as DefaultRoutePoolOverview['targetAgentId'],
      native,
      shared,
    );
    return [{ agentId: target, surface: native, dialect: mockWriterDialect(target) }];
  }
  if (kind === 'oauth') return [];
  const surfaces: RoutePoolSurface[] = agentId === 'workbuddy'
    ? ['chat_completions']
    : agentId === 'zcode' || agentId === 'pi'
      ? ['messages', 'responses', 'chat_completions']
      : [];
  return MOCK_POOL_WRITERS
    .filter((writer) => {
      const surface = mockWriterSurface(writer);
      if (surface == null || !surfaces.includes(surface)) return false;
      if (shared && surface === 'chat_completions') return writer === 'kimi';
      return true;
    })
    .map((writer) => ({
      agentId: writer,
      surface: mockWriterSurface(writer)!,
      dialect: writer,
    }));
}

export function restoreMockRouteMembership(payload: RouteMembershipTrashPayload): void {
  for (const state of adapterStates) {
    for (const snapshot of payload.members) {
      const pool = state.defaultPools.find((item) => item.id === snapshot.routePoolId);
      if (!pool) continue;
      if (pool.members.some((member) => (
        member.sourceKind === payload.sourceKind && member.sourceId === payload.sourceId
      ))) {
        continue;
      }
      pool.members.push({
        sourceKind: payload.sourceKind,
        sourceId: payload.sourceId,
        enabled: snapshot.enabled,
        priority: snapshot.priority,
      });
    }
  }
}

export function resetMockAdapters(): void {
  adapterStates.forEach((state) => {
    state.generatedProviders.forEach((provider) => state.removeGeneratedProvider?.(provider));
    state.profiles.length = 0;
    state.bridgeStatuses.clear();
    state.generatedProviders.clear();
    state.routePoolV2 = true;
    state.shareChatCompletions = false;
    state.defaultPools.length = 0;
    state.localTokens.clear();
    state.localTokenNames.clear();
    state.extraLocalTokens.length = 0;
    state.hiddenPrimaryIds.clear();
  });
}

/** Snapshot of profiles across all mock adapter ports (ticket wallet). */
export function listMockAdapterProfiles(): AdapterProfile[] {
  const out: AdapterProfile[] = [];
  for (const state of adapterStates) {
    for (const profile of state.profiles) {
      out.push({ ...profile });
    }
  }
  return out;
}

/**
 * Unbind helper: stop bridge + drop generated projection even when it is current.
 * Ticket source rows stay in the wallet.
 */
export function removeMockAdapterBinding(profileId: string): void {
  for (const state of adapterStates) {
    const index = state.profiles.findIndex((profile) => profile.id === profileId);
    if (index < 0) continue;
    const profile = state.profiles[index];
    const providerId = profile.generatedProviderId;
    const generated = providerId
      ? state.generatedProviders.get(providerId)
      : undefined;
    if (generated) {
      state.removeGeneratedProvider?.(generated);
      state.generatedProviders.delete(generated.id);
    }
    state.bridgeStatuses.delete(profileId);
    state.profiles.splice(index, 1);
    return;
  }
  throw adapterCommandError({
    code: 'not_found',
    message: `adapter profile not found: ${profileId}`,
    retryable: false,
  });
}

/** Sync bridge status lookup for ticket wallet bindings. */
export function getMockBridgeStatusSync(profileId: string): AdapterBridgeRuntimeStatus | undefined {
  for (const state of adapterStates) {
    const status = state.bridgeStatuses.get(profileId);
    if (status) return { ...status };
  }
  return undefined;
}

/**
 * Test / fixture helper: insert profiles + optional bridge statuses into every
 * live mock adapter port (normally one per createBackend()).
 */
export function seedMockAdapterProfiles(
  profiles: readonly AdapterProfile[],
  bridges?: ReadonlyMap<string, AdapterBridgeRuntimeStatus> | Record<string, AdapterBridgeRuntimeStatus>,
): void {
  const bridgeEntries = bridges instanceof Map
    ? [...bridges.entries()]
    : Object.entries(bridges ?? {});
  for (const state of adapterStates) {
    for (const profile of profiles) {
      const existing = state.profiles.findIndex((item) => item.id === profile.id);
      if (existing >= 0) state.profiles[existing] = { ...profile };
      else state.profiles.push({ ...profile });
    }
    for (const [id, status] of bridgeEntries) {
      state.bridgeStatuses.set(id, { ...status });
    }
  }
}

/** Override the Routes pool product flag. Mock default matches production (on). */
export function setMockRoutePoolV2(enabled: boolean): void {
  for (const state of adapterStates) {
    state.routePoolV2 = enabled;
  }
}

export function seedMockDefaultRoutePools(pools: readonly DefaultRoutePoolOverview[]): void {
  for (const state of adapterStates) {
    state.defaultPools = pools.map((pool) => ({
      ...pool,
      members: pool.members.map((member) => ({ ...member })),
      listedModels: [...(pool.listedModels ?? [])],
    }));
  }
}

function applyBindingToState(
  state: MockAdapterState,
  request: AdapterApplyRequest,
): AdapterApplyResult {
  const resolver = state.resolver;
  const plan = buildPlan(resolver, request);
  if (!plan.canApply) {
    throw adapterCommandError({
      code: 'unsupported',
      message: '当前适配路径尚不可应用',
      retryable: false,
    });
  }
  const existing = state.profiles.find(
    (profile) =>
      profile.sourceKind === request.sourceKind &&
      profile.sourceId === request.sourceId &&
      profile.targetAgentId === request.targetAgentId,
  );
  const now = new Date().toISOString();
  const { profile, provider } = materializeApply(request, plan, existing, now);
  if (!existing) state.profiles.push(profile);
  if (plan.analysis.route === 'local_bridge') {
    state.bridgeStatuses.set(profile.id, runningBridgeStatus(profile));
  }
  const generated = resolver.upsertGeneratedProvider?.(provider) ?? provider;
  state.generatedProviders.set(generated.id, { ...generated });
  return {
    profile: { ...profile },
    provider: { ...generated },
  };
}

/** Same write as AdapterPort.apply (buildPlan + materializeApply), without delay. */
export function seedAppliedBinding(request: AdapterApplyRequest): AdapterApplyResult {
  let last: AdapterApplyResult | undefined;
  for (const state of adapterStates) {
    last = applyBindingToState(state, request);
  }
  if (!last) {
    throw adapterCommandError({
      code: 'not_found',
      message: 'no live mock adapter to apply into',
      retryable: false,
    });
  }
  return last;
}

export function createMockAdapterPort(resolver: MockAdapterSourceResolver): AdapterPort {
  const state: MockAdapterState = {
    profiles: [],
    bridgeStatuses: new Map(),
    generatedProviders: new Map(),
    resolver,
    removeGeneratedProvider: resolver.removeGeneratedProvider,
    routePoolV2: true,
    shareChatCompletions: false,
    defaultPools: [],
    localTokens: new Map(),
    localTokenNames: new Map(),
    extraLocalTokens: [],
    hiddenPrimaryIds: new Set(),
    localGatewayRunning: false,
    localGatewayPort: null,
    sourceModelCatalogs: new Map(),
    routeTraces: seedMockRouteTraces(),
  };
  adapterStates.add(state);

  return {
    async analyze(request) {
      await delay(20);
      return analyze(resolver, request);
    },
    async plan(request) {
      await delay(20);
      return buildPlan(resolver, request);
    },
    async listProfiles(filter: AdapterProfileFilter = {}) {
      await delay(20);
      return state.profiles
        .filter((profile) => !filter.sourceKind || profile.sourceKind === filter.sourceKind)
        .filter((profile) => !filter.sourceId || profile.sourceId === filter.sourceId)
        .filter((profile) => !filter.targetAgentId || profile.targetAgentId === filter.targetAgentId)
        .filter((profile) => !filter.mode || profile.mode === filter.mode)
        .filter((profile) => !filter.route || profile.route === filter.route)
        .filter((profile) => !filter.status || profile.status === filter.status)
        .filter((profile) => filter.autoStart == null || profile.autoStart === filter.autoStart)
        .map((profile) => ({ ...profile }));
    },
    async listDefaultRoutePools() {
      await delay(20);
      if (!state.routePoolV2) return { enabled: false, pools: [], chatCompletionsShared: false };
      return {
        enabled: true,
        chatCompletionsShared: state.shareChatCompletions,
        pools: state.defaultPools.map((pool) => ({
          ...pool,
          members: pool.members.map((member) => ({ ...member })),
          listedModels: [...(pool.listedModels ?? [])],
        })),
      };
    },
    async listLocalTokens() {
      await delay(20);
      if (!state.routePoolV2) return [];
      const primaries = state.defaultPools.flatMap((pool) => {
        if (state.hiddenPrimaryIds.has(pool.id)) return [];
        const existing = state.localTokens.get(pool.id);
        const token = existing?.trim() || `ahb_${pool.id.replace(/[^a-zA-Z0-9]/g, '').slice(0, 12) || 'token'}`;
        if (!existing) state.localTokens.set(pool.id, token);
        return [{
          id: pool.id,
          poolId: pool.id,
          token,
          name: state.localTokenNames.get(pool.id) ?? '',
          primary: true,
        }];
      });
      const extras = state.extraLocalTokens
        .filter((row) => state.defaultPools.some((pool) => pool.id === row.poolId))
        .map((row) => ({ ...row, primary: false }));
      return [...primaries, ...extras];
    },
    async ensureSourceModelCatalog(_sourceKind, sourceId) {
      await delay(20);
      const cached = state.sourceModelCatalogs.get(sourceId);
      if (cached) return cached;
      const models = state.defaultPools.flatMap((pool) => pool.listedModels ?? []);
      const unique = [...new Set(models.map((item) => item.trim()).filter(Boolean))];
      return {
        models: unique,
        source: unique.length > 0 ? 'live' as const : 'empty' as const,
        canCustomize: unique.length === 0,
      };
    },
    async setSourceCustomModels(_sourceKind, sourceId, models) {
      await delay(20);
      const unique = [...new Set(models.map((item) => item.trim()).filter(Boolean))];
      const catalog = {
        models: unique,
        source: 'custom' as const,
        canCustomize: true,
      };
      state.sourceModelCatalogs.set(sourceId, catalog);
      return catalog;
    },
    async setLocalTokenCustomModels(token, models) {
      await delay(20);
      const unique = [...new Set(models.map((item) => item.trim()).filter(Boolean))];
      if (token.trim()) state.sourceModelCatalogs.set(token, {
        models: unique,
        source: 'custom',
        canCustomize: true,
      });
      return unique;
    },
    async listLocalTokenModels(token) {
      await delay(20);
      if (!token.trim()) return [];
      const models = state.defaultPools.flatMap((pool) => pool.listedModels ?? []);
      const unique = [...new Set(models.map((item) => item.trim()).filter(Boolean))];
      return unique.length > 0 ? unique : ['gpt-5.4'];
    },
    async refreshLocalTokenModels(token) {
      await delay(20);
      if (!token.trim()) return [];
      const poolId = [...state.localTokens.entries()].find(([, value]) => value === token)?.[0];
      const pool = state.defaultPools.find((item) => item.id === poolId);
      const fromLogins = (pool?.members ?? []).flatMap((member) => (
        state.sourceModelCatalogs.get(member.sourceId)?.models ?? []
      ));
      const unique = [...new Set(fromLogins.map((item) => item.trim()).filter(Boolean))];
      if (unique.length > 0) return unique;
      const models = pool?.listedModels ?? state.defaultPools.flatMap((item) => item.listedModels ?? []);
      const listed = [...new Set(models.map((item) => item.trim()).filter(Boolean))];
      return listed.length > 0 ? listed : ['gpt-5.4'];
    },
    async testLocalToken(endpoint, token, path, model) {
      await delay(20);
      const trimmedToken = token.trim();
      const trimmedEndpoint = endpoint.trim();
      const trimmedPath = path.trim() || '/v1/chat/completions';
      if (!trimmedToken || !trimmedEndpoint) {
        return {
          outcome: 'invalid',
          httpStatus: null,
          latencyMs: 0,
          upstreamStatus: null,
          requestUrl: null,
          requestMethod: null,
          requestBody: null,
          responseBody: null,
          errorMessage: null,
        };
      }
      const loopback = trimmedEndpoint.includes('127.0.0.1')
        || trimmedEndpoint.includes('localhost')
        || trimmedEndpoint.includes('[::1]');
      if (!loopback) {
        return {
          outcome: 'invalid',
          httpStatus: null,
          latencyMs: 0,
          upstreamStatus: null,
          requestUrl: null,
          requestMethod: null,
          requestBody: null,
          responseBody: null,
          errorMessage: null,
        };
      }
      const requestUrl = trimmedEndpoint.includes('://')
        ? trimmedEndpoint.replace(/\/[^/]*$/, trimmedPath)
        : `http://${trimmedEndpoint}${trimmedPath}`;
      return {
        outcome: 'ok',
        httpStatus: 200,
        latencyMs: 4,
        upstreamStatus: null,
        requestUrl,
        requestMethod: 'POST',
        requestBody: JSON.stringify({
          model: model?.trim() || 'mock',
          messages: [{ role: 'user', content: 'ping' }],
          max_tokens: 8,
          stream: false,
        }),
        responseBody: '{"choices":[{"message":{"content":"ok"}}]}',
        errorMessage: null,
      };
    },
    async setLocalToken(poolId, token) {
      await delay(20);
      const trimmed = token.trim();
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      if (!trimmed) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'entry key must not be empty',
          retryable: false,
        });
      }
      const extra = state.extraLocalTokens.find((row) => row.id === poolId);
      if (extra) {
        extra.token = trimmed;
        return { id: extra.id, poolId: extra.poolId, token: trimmed, name: extra.name, primary: false };
      }
      if (!state.defaultPools.some((pool) => pool.id === poolId)) {
        throw adapterCommandError({
          code: 'not_found',
          message: `route pool not found: ${poolId}`,
          retryable: false,
        });
      }
      state.localTokens.set(poolId, trimmed);
      state.hiddenPrimaryIds.delete(poolId);
      return {
        id: poolId,
        poolId,
        token: trimmed,
        name: state.localTokenNames.get(poolId) ?? '',
        primary: true,
      };
    },
    async createLocalToken(poolId, name) {
      await delay(20);
      const trimmedName = name.trim();
      if (!state.routePoolV2) {
        throw adapterCommandError({ code: 'unsupported', message: 'route_pool_v2 is disabled', retryable: false });
      }
      if (!trimmedName) {
        throw adapterCommandError({ code: 'invalid_arg', message: 'entry key name must not be empty', retryable: false });
      }
      if (!state.defaultPools.some((pool) => pool.id === poolId)) {
        throw adapterCommandError({
          code: 'not_found',
          message: `route pool not found: ${poolId}`,
          retryable: false,
        });
      }
      const token = `ahb_${Math.random().toString(36).slice(2, 12)}`;
      const row = { id: `extra-${state.extraLocalTokens.length + 1}`, poolId, name: trimmedName, token };
      state.extraLocalTokens.push(row);
      return { ...row, primary: false };
    },
    async setLocalTokenName(id, name) {
      await delay(20);
      const trimmedName = name.trim();
      if (!trimmedName) {
        throw adapterCommandError({ code: 'invalid_arg', message: 'entry key name must not be empty', retryable: false });
      }
      const extra = state.extraLocalTokens.find((row) => row.id === id);
      if (extra) {
        extra.name = trimmedName;
        return { id: extra.id, poolId: extra.poolId, token: extra.token, name: extra.name, primary: false };
      }
      if (!state.defaultPools.some((pool) => pool.id === id)) {
        throw adapterCommandError({ code: 'not_found', message: `route pool not found: ${id}`, retryable: false });
      }
      const existing = state.localTokens.get(id);
      const token = existing?.trim() || `ahb_${id.replace(/[^a-zA-Z0-9]/g, '').slice(0, 12) || 'token'}`;
      if (!existing) state.localTokens.set(id, token);
      state.localTokenNames.set(id, trimmedName);
      return { id, poolId: id, token, name: trimmedName, primary: true };
    },
    async deleteLocalToken(id) {
      await delay(20);
      const extraIndex = state.extraLocalTokens.findIndex((row) => row.id === id);
      const extra = extraIndex >= 0 ? state.extraLocalTokens[extraIndex] : undefined;
      const poolId = extra?.poolId
        ?? (state.defaultPools.some((pool) => pool.id === id) ? id : null);
      if (!poolId) {
        throw adapterCommandError({
          code: 'not_found',
          message: `entry key not found: ${id}`,
          retryable: false,
        });
      }
      const extraCount = state.extraLocalTokens.filter((row) => row.poolId === poolId).length;
      const listed = extraCount + (state.hiddenPrimaryIds.has(poolId) ? 0 : 1);
      if (listed <= 1) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'cannot delete the only entry key for this type',
          retryable: false,
        });
      }
      if (extraIndex >= 0) {
        state.extraLocalTokens.splice(extraIndex, 1);
        return;
      }
      const promote = state.extraLocalTokens.find((row) => row.poolId === id);
      if (!promote) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'cannot delete the only entry key for this type',
          retryable: false,
        });
      }
      state.localTokens.set(id, promote.token);
      state.localTokenNames.set(id, promote.name);
      state.hiddenPrimaryIds.delete(id);
      state.extraLocalTokens.splice(state.extraLocalTokens.indexOf(promote), 1);
    },
    async setChatCompletionsShared(shared: boolean) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      if (state.shareChatCompletions !== shared) {
        state.shareChatCompletions = shared;
        if (shared) mockMergeChatPools(state);
        else mockSplitChatPools(state);
      }
      return this.listDefaultRoutePools();
    },
    async attachPoolOwnedAuthorization(request) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      if (request.sourceKind === 'provider') {
        const provider = getMockProviderById(request.sourceId);
        if (!provider) {
          throw adapterCommandError({
            code: 'not_found',
            message: `provider not found: ${request.sourceId}`,
            retryable: false,
          });
        }
        if (provider.agentId !== request.targetAgentId) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'authorization does not belong to this Agent',
            retryable: false,
          });
        }
        if (provider.isCurrent) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'the live login cannot be pool-only',
            retryable: false,
          });
        }
        upsertMockProvider({ ...provider, home: 'route_pool' });
      } else {
        const account = getMockAccountById(request.sourceId);
        if (!account) {
          throw adapterCommandError({
            code: 'not_found',
            message: `account not found: ${request.sourceId}`,
            retryable: false,
          });
        }
        if (account.agentId !== request.targetAgentId) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'authorization does not belong to this Agent',
            retryable: false,
          });
        }
        if (account.isCurrent) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'the live login cannot be pool-only',
            retryable: false,
          });
        }
        upsertMockAccount({ ...account, home: 'route_pool' });
      }
      const surface: RoutePoolSurface = request.surface;
      const poolAgent = mockChatWriter(request.targetAgentId, surface, state.shareChatCompletions);
      const dialect: RoutePoolDialect =
        poolAgent === 'claude'
        || poolAgent === 'codex'
        || poolAgent === 'grok'
        || poolAgent === 'kimi'
        || poolAgent === 'dsh'
          ? poolAgent
          : 'generic';
      let pool = state.defaultPools.find((item) => (
        item.targetAgentId === poolAgent && item.surface === surface
      ));
      if (!pool) {
        pool = {
          id: `pool-${poolAgent}-${surface}`,
          targetAgentId: poolAgent,
          surface,
          dialect,
          unifiedGatewayEnrolled: false,
          members: [],
          listedModels: [],
        };
        state.defaultPools.push(pool);
      }
      if (!pool.members.some((member) => (
        member.sourceKind === request.sourceKind && member.sourceId === request.sourceId
      ))) {
        pool.members.push({
          sourceKind: request.sourceKind,
          sourceId: request.sourceId,
          enabled: true,
        });
      }
      return {
        ...pool,
        members: pool.members.map((member) => ({ ...member })),
        listedModels: [...(pool.listedModels ?? [])],
      };
    },
    async forkConnectionAuthorization(sourceKind, sourceId) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      if (sourceKind !== 'account') {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'only official logins can be copied for pool editing',
          retryable: false,
        });
      }
      const account = getMockAccountById(sourceId);
      if (!account) {
        throw adapterCommandError({
          code: 'not_found',
          message: `account not found: ${sourceId}`,
          retryable: false,
        });
      }
      if (account.kind !== 'oauth') {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'only official logins can be copied for pool editing',
          retryable: false,
        });
      }
      if (account.home === 'route_pool') {
        const result: ForkedConnectionAuthorization = {
          sourceKind,
          sourceId,
          originalSourceId: sourceId,
          copied: false,
        };
        return result;
      }
      const members = state.defaultPools.flatMap((pool) => pool.members.filter((member) => (
        member.sourceKind === sourceKind && member.sourceId === sourceId
      )));
      if (members.length === 0) {
        throw adapterCommandError({
          code: 'not_found',
          message: `route authorization not found: ${sourceId}`,
          retryable: false,
        });
      }
      const copy = upsertMockAccount({
        ...account,
        id: `${account.agentId}-acc-${Date.now()}-${Math.random().toString(16).slice(2)}`,
        isCurrent: false,
        home: 'route_pool',
      });
      for (const pool of state.defaultPools) {
        for (const member of pool.members) {
          if (member.sourceKind === sourceKind && member.sourceId === sourceId) {
            member.sourceId = copy.id;
          }
        }
      }
      return {
        sourceKind,
        sourceId: copy.id,
        originalSourceId: sourceId,
        copied: true,
      };
    },
    async setRouteAuthorizationEnabled(sourceKind, sourceId, enabled) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      let changed = 0;
      for (const pool of state.defaultPools) {
        for (const member of pool.members) {
          if (member.sourceKind !== sourceKind || member.sourceId !== sourceId) continue;
          if (member.enabled === enabled) continue;
          member.enabled = enabled;
          changed += 1;
        }
      }
      return changed;
    },
    async setRouteAuthorizationPriority(sourceKind, sourceId, priority) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      let changed = 0;
      for (const pool of state.defaultPools) {
        for (const member of pool.members) {
          if (member.sourceKind !== sourceKind || member.sourceId !== sourceId) continue;
          if (member.priority === priority) continue;
          member.priority = priority;
          changed += 1;
        }
      }
      return changed;
    },
    async recycleRouteMembership(sourceKind, sourceId) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      const members: RouteMembershipTrashPayload['members'] = [];
      for (const pool of state.defaultPools) {
        for (const member of pool.members) {
          if (member.sourceKind !== sourceKind || member.sourceId !== sourceId) continue;
          members.push({
            routePoolId: pool.id,
            enabled: member.enabled,
            priority: member.priority ?? 0,
            position: 0,
          });
        }
      }
      const account = sourceKind === 'account' ? getMockAccountById(sourceId) : undefined;
      const provider = sourceKind === 'provider' ? getMockProviderById(sourceId) : undefined;
      const agentId = account?.agentId ?? provider?.agentId ?? state.defaultPools[0]?.targetAgentId;
      if (!agentId) {
        throw adapterCommandError({
          code: 'not_found',
          message: `route authorization not found: ${sourceId}`,
          retryable: false,
        });
      }
      moveMockMembershipToTrash(agentId, account?.label ?? provider?.name ?? sourceId, {
        sourceKind,
        sourceId,
        members,
      });
      return this.removeRouteAuthorization(sourceKind, sourceId);
    },
    async removeRouteAuthorization(sourceKind, sourceId) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      let removed = 0;
      for (const pool of state.defaultPools) {
        const before = pool.members.length;
        pool.members = pool.members.filter((member) => (
          member.sourceKind !== sourceKind || member.sourceId !== sourceId
        ));
        removed += before - pool.members.length;
      }
      return removed;
    },
    async syncConnectionAuthorizations(request) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      let added = 0;
      let skipped = 0;
      const enroll = (
        agentId: string,
        sourceKind: 'account' | 'provider',
        sourceId: string,
        home?: 'route_pool',
        kind: 'oauth' | 'apikey' = 'apikey',
      ) => {
        if (home === 'route_pool') {
          skipped += 1;
          return;
        }
        const targets = mockPoolTargetsForSync(agentId, kind, state.shareChatCompletions);
        if (targets.length === 0) {
          skipped += 1;
          return;
        }
        let addedAny = false;
        for (const target of targets) {
          let pool = state.defaultPools.find((item) => (
            item.targetAgentId === target.agentId && item.surface === target.surface
          ));
          if (!pool) {
            pool = {
              id: `pool-${target.agentId}-${target.surface}`,
              targetAgentId: target.agentId,
              surface: target.surface,
              dialect: target.dialect,
              unifiedGatewayEnrolled: false,
              members: [],
              listedModels: [],
            };
            state.defaultPools.push(pool);
          }
          if (pool.members.some((member) => (
            member.sourceKind === sourceKind && member.sourceId === sourceId
          ))) {
            continue;
          }
          pool.members.push({ sourceKind, sourceId, enabled: true });
          addedAny = true;
        }
        if (addedAny) added += 1;
        else skipped += 1;
      };
      const accounts = listMockAccounts();
      const providers = listMockProviders();
      if (request) {
        for (const source of request.sources) {
          if (source.sourceKind === 'account') {
            const account = accounts.find((item) => item.id === source.sourceId);
            if (account) {
              enroll(account.agentId, 'account', account.id, account.home, account.kind);
            } else skipped += 1;
            continue;
          }
          const provider = providers.find((item) => item.id === source.sourceId);
          if (!provider) {
            skipped += 1;
            continue;
          }
          if (state.generatedProviders.has(provider.id)) {
            skipped += 1;
            continue;
          }
          enroll(provider.agentId, 'provider', provider.id, provider.home, 'apikey');
        }
      } else {
        for (const account of accounts) {
          enroll(account.agentId, 'account', account.id, account.home, account.kind);
        }
        for (const provider of providers) {
          if (state.generatedProviders.has(provider.id)) {
            skipped += 1;
            continue;
          }
          enroll(provider.agentId, 'provider', provider.id, provider.home, 'apikey');
        }
      }
      return { added, skipped };
    },
    async enrollNativeToGateway(profileId) {
      await delay(20);
      if (!state.routePoolV2) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'route_pool_v2 is disabled',
          retryable: false,
        });
      }
      const profile = state.profiles.find((item) => item.id === profileId);
      if (!profile) {
        throw adapterCommandError({
          code: 'not_found',
          message: `adapter profile not found: ${profileId}`,
          retryable: false,
        });
      }
      if (profile.route !== 'native_endpoint' && profile.route !== 'config_sync') {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'already a local route',
          retryable: false,
        });
      }
      const plan = buildPlan(resolver, {
        sourceKind: profile.sourceKind,
        sourceId: profile.sourceId,
        targetAgentId: profile.targetAgentId,
      });
      if (!plan.canApply || plan.analysis.route !== 'local_bridge') {
        throw adapterCommandError({
          code: 'unsupported',
          message: plan.reason || 'this login cannot use the local gateway for that tool',
          retryable: false,
        });
      }
      profile.route = 'local_bridge';
      profile.localPort = profile.localPort ?? 43121;
      profile.updatedAt = new Date().toISOString();
      state.bridgeStatuses.set(profile.id, runningBridgeStatus(profile));
      const surface = profile.targetAgentId === 'claude'
        ? 'messages' as const
        : profile.targetAgentId === 'kimi' || profile.targetAgentId === 'dsh'
          ? 'chat_completions' as const
          : 'responses' as const;
      const dialect: DefaultRoutePoolOverview['dialect'] =
        profile.targetAgentId === 'claude'
        || profile.targetAgentId === 'codex'
        || profile.targetAgentId === 'grok'
        || profile.targetAgentId === 'kimi'
        || profile.targetAgentId === 'dsh'
          ? profile.targetAgentId
          : 'generic';
      const overview: DefaultRoutePoolOverview = {
        id: profile.id,
        targetAgentId: profile.targetAgentId,
        surface,
        dialect,
        unifiedGatewayEnrolled: true,
        gatewayPort: profile.localPort ?? 43121,
        members: [{
          sourceKind: profile.sourceKind,
          sourceId: profile.sourceId,
          enabled: true,
        }],
        listedModels: [],
      };
      const existing = state.defaultPools.findIndex((pool) => pool.id === overview.id);
      if (existing >= 0) state.defaultPools[existing] = overview;
      else state.defaultPools.push(overview);
      return { ...overview, members: overview.members.map((member) => ({ ...member })) };
    },
    async apply(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
      await delay(20);
      return applyBindingToState(state, request);
    },
    async remove(profileId: string) {
      await delay(20);
      const index = state.profiles.findIndex((profile) => profile.id === profileId);
      if (index < 0) {
        throw adapterCommandError({
          code: 'not_found',
          message: `adapter profile not found: ${profileId}`,
          retryable: false,
        });
      }
      const profile = state.profiles[index];
      const providerId = profile.generatedProviderId;
      const generated = providerId
        ? resolver.getProviderById(providerId) ?? state.generatedProviders.get(providerId)
        : undefined;
      if (!generated) {
        throw adapterCommandError({
          code: 'not_found',
          message: '适配生成的 Connection 不存在，无法安全删除',
          retryable: false,
        });
      }
      resolver.removeGeneratedProvider?.(generated);
      state.generatedProviders.delete(generated.id);
      state.bridgeStatuses.delete(profileId);
      state.profiles.splice(index, 1);
    },
    async startBridge(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const status = runningBridgeStatus(profile);
      state.bridgeStatuses.set(profileId, status);
      return { ...status };
    },
    async stopBridge(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const current = state.bridgeStatuses.get(profileId);
      const status: AdapterBridgeRuntimeStatus = {
        profileId,
        state: 'stopped',
        port: profile.localPort ?? current?.port ?? null,
        endpoint: profile.localPort ? `http://127.0.0.1:${profile.localPort}/v1` : null,
        startedAt: current?.startedAt ?? null,
        upstreamStatus: 'stopped',
        recentInbound: current?.recentInbound ?? [],
        totalRequestCount: current?.totalRequestCount ?? 0,
        failedRequestCount: current?.failedRequestCount ?? 0,
        lastRequestAt: current?.lastRequestAt ?? null,
      };
      state.bridgeStatuses.set(profileId, status);
      return { ...status };
    },
    async getBridgeStatus(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const status = state.bridgeStatuses.get(profileId) ?? {
        profileId,
        state: 'stopped' as const,
        port: profile.localPort ?? null,
        endpoint: profile.localPort ? `http://127.0.0.1:${profile.localPort}/v1` : null,
        startedAt: null,
        upstreamStatus: 'stopped',
        recentInbound: [],
        totalRequestCount: 0,
        failedRequestCount: 0,
        lastRequestAt: null,
      };
      return { ...status, recentInbound: [...(status.recentInbound ?? [])] };
    },
    async setBridgeAutoStart(profileId, autoStart) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      profile.autoStart = autoStart;
      profile.updatedAt = new Date().toISOString();
      return { ...profile };
    },
    async startLocalGateway() {
      await delay(20);
      state.localGatewayRunning = true;
      state.localGatewayPort = 43121;
      for (const pool of state.defaultPools) {
        pool.gatewayPort = 43121;
      }
      return mockLocalGatewayStatus(state);
    },
    async stopLocalGateway() {
      await delay(20);
      state.localGatewayRunning = false;
      state.localGatewayPort = null;
      return mockLocalGatewayStatus(state);
    },
    async getLocalGatewayStatus() {
      await delay(20);
      return mockLocalGatewayStatus(state);
    },
    async queryRouteTraces(query = {}) {
      await delay(20);
      return queryMockRouteTraces(state.routeTraces, query);
    },
    async deleteRouteTraces(requestIds) {
      await delay(20);
      const want = new Set(requestIds.map((id) => id.trim()).filter(Boolean));
      const before = state.routeTraces.length;
      state.routeTraces = state.routeTraces.filter((row) => !want.has(row.requestId));
      return { deleted: before - state.routeTraces.length };
    },
  };
}

function mockLocalGatewayStatus(state: MockAdapterState): LocalGatewayStatus {
  return {
    running: state.localGatewayRunning,
    port: state.localGatewayPort,
    restarting: false,
    statuses: state.localGatewayRunning
      ? state.defaultPools.map((pool) => ({
        profileId: pool.id,
        state: 'running' as const,
        port: 43121,
        endpoint: 'http://127.0.0.1:43121/v1',
        startedAt: '2026-08-12T00:00:00.000Z',
        upstreamStatus: 'connected' as const,
        recentInbound: [],
        recentRouteTraces: mockRouteTraces(),
        totalRequestCount: 0,
        failedRequestCount: 0,
        lastRequestAt: null,
        localToken: `ahb_${pool.id.slice(0, 8)}`,
      }))
      : [],
    unauthenticatedTraces: state.localGatewayRunning
      ? [{
        traceVersion: 2,
        requestId: 'mock-req-unauth',
        at: '2026-08-12T00:00:00.000Z',
        method: 'POST',
        path: '/v1/messages',
        httpStatus: 401,
        ok: false,
        localAuth: { status: 'failed', code: 'invalid_api_key' },
        pool: { status: 'skipped' },
        conversion: { status: 'skipped', path: '' },
        upstreamAuth: { status: 'skipped' },
        upstream: { status: 'skipped' },
        failureStage: 'local_auth',
      }]
      : [],
  };
}

function localBridgeProfile(state: MockAdapterState, profileId: string): AdapterProfile {
  const profile = state.profiles.find((item) => item.id === profileId);
  if (!profile) {
    throw adapterCommandError({
      code: 'not_found',
      message: `adapter profile not found: ${profileId}`,
      retryable: false,
    });
  }
  if (profile.route !== 'local_bridge') {
    throw adapterCommandError({
      code: 'unsupported',
      message: '这条接法不需要本机转发',
      retryable: false,
    });
  }
  return profile;
}

function mockInboundRows(): AdapterBridgeRuntimeStatus['recentInbound'] {
  return [
    {
      at: '2026-08-12T00:00:02.000Z',
      method: 'POST',
      path: '/v1/responses',
      status: 200,
      ok: true,
    },
    {
      at: '2026-08-12T00:00:01.000Z',
      method: 'GET',
      path: '/models',
      status: 200,
      ok: true,
    },
  ];
}

function mockRouteTraces(): AdapterBridgeRuntimeStatus['recentRouteTraces'] {
  return [
    {
      traceVersion: 2,
      requestId: 'mock-req-ok',
      at: '2026-08-12T00:00:02.000Z',
      method: 'POST',
      path: '/v1/responses',
      httpStatus: 200,
      ok: true,
      model: 'gpt-5',
      latencyMs: 842,
      ttftMs: 210,
      inputTokens: 1200,
      outputTokens: 340,
      localAuth: { status: 'ok', profileId: 'mock-profile', port: 32123, keyLast4: '1234' },
      pool: {
        status: 'ok',
        selectedMember: { label: 'pool-acct-1', sourceKind: 'account', sourceId: 'acct-1' },
      },
      conversion: { status: 'ok', path: 'responses_to_codex_responses', result: 'converted' },
      upstreamAuth: { status: 'ok', httpStatus: 200 },
      upstream: {
        status: 'ok',
        url: 'https://api.openai.com/v1/responses',
        member: { label: 'pool-acct-1', sourceKind: 'account', sourceId: 'acct-1' },
        upstreamModel: 'gpt-5',
        httpStatus: 200,
      },
    },
    {
      traceVersion: 2,
      requestId: 'mock-req-fail',
      at: '2026-08-12T00:00:01.000Z',
      method: 'POST',
      path: '/v1/messages',
      httpStatus: 401,
      ok: false,
      model: 'claude-sonnet',
      latencyMs: 12,
      ttftMs: null,
      inputTokens: 80,
      outputTokens: 0,
      localAuth: { status: 'ok', profileId: 'mock-profile', port: 32123, keyLast4: '5678' },
      pool: {
        status: 'ok',
        selectedMember: { label: 'pool-acct-2', sourceKind: 'account', sourceId: 'acct-2' },
      },
      conversion: { status: 'ok', path: 'messages_to_anthropic', result: 'converted' },
      upstreamAuth: { status: 'failed', httpStatus: 401, code: 'unauthorized' },
      upstream: {
        status: 'failed',
        url: 'https://api.anthropic.com/v1/messages',
        member: { label: 'pool-acct-2', sourceKind: 'account', sourceId: 'acct-2' },
        httpStatus: 401,
        code: 'unauthorized',
      },
      failureStage: 'upstream_response',
    },
  ];
}

function seedMockRouteTraces(): AdapterBridgeRouteTrace[] {
  const base = mockRouteTraces() ?? [];
  const template = base[0];
  if (!template) return [];
  const extra: AdapterBridgeRouteTrace[] = Array.from({ length: 60 }, (_, index) => ({
    ...template,
    requestId: `mock-req-extra-${index}`,
    at: new Date(Date.parse('2026-08-11T23:59:00.000Z') - index * 1000).toISOString(),
    path: index % 3 === 0 ? '/v1/messages' : index % 3 === 1 ? '/v1/chat/completions' : '/v1/responses',
    conversion: {
      ...template.conversion,
      path: index % 3 === 0
        ? 'messages_to_anthropic'
        : index % 3 === 1
          ? 'chat_to_openai_chat'
          : index % 5 === 0
            ? 'responses_to_grok'
            : 'responses_to_codex_responses',
    },
    localAuth: {
      ...template.localAuth,
      keyLast4: index % 2 === 0 ? '1234' : '5678',
    },
  }));
  return [...base, ...extra];
}

function queryMockRouteTraces(
  rows: readonly AdapterBridgeRouteTrace[],
  query: RouteTraceQuery,
): RouteTracePage {
  const keyLast4 = query.keyLast4?.trim() || '';
  const poolId = query.poolId?.trim() || '';
  const routeId = query.routeId?.trim() || '';
  const endpointKind = query.endpointKind ?? null;
  const failedOnly = query.failedOnly === true;
  const offset = Math.max(0, query.offset ?? 0);
  const limit = Math.min(100, Math.max(1, query.limit ?? 50));
  const filtered = rows.filter((row) => {
    if (keyLast4 && row.localAuth.keyLast4 !== keyLast4) return false;
    if (!keyLast4 && poolId && (row.localAuth.profileId ?? row.profileId) !== poolId) return false;
    if (routeId && row.profileId !== routeId) return false;
    if (failedOnly && row.ok) return false;
    if (endpointKind) {
      const path = row.path;
      const conversion = (row.conversion.path ?? '').toLowerCase();
      const upstream = (row.upstream.url ?? '').toLowerCase();
      const grok = conversion.includes('grok') || upstream.includes('grok');
      if (endpointKind === 'messages' && !path.startsWith('/v1/messages')) return false;
      if (endpointKind === 'chat_completions' && !path.startsWith('/v1/chat/completions')) return false;
      if (endpointKind === 'responses_grok' && !(path.startsWith('/v1/responses') && grok)) return false;
      if (endpointKind === 'responses_codex' && !(path.startsWith('/v1/responses') && !grok)) return false;
    }
    return true;
  }).slice().sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0));
  return {
    rows: filtered.slice(offset, offset + limit),
    total: filtered.length,
    offset,
    limit,
  };
}

function runningBridgeStatus(profile: AdapterProfile): AdapterBridgeRuntimeStatus {
  const port = profile.localPort ?? 32123;
  const recentInbound = mockInboundRows() ?? [];
  const recentRouteTraces = mockRouteTraces() ?? [];
  return {
    profileId: profile.id,
    state: 'running',
    port,
    endpoint: `http://127.0.0.1:${port}/v1`,
    startedAt: new Date().toISOString(),
    upstreamStatus: 'unknown',
    recentInbound,
    recentRouteTraces,
    totalRequestCount: 42,
    failedRequestCount: 3,
    lastRequestAt: recentInbound[0]?.at ?? null,
  };
}


export type { MockAdapterSourceResolver } from './adapter/types';
export {
  DEV_MOCK_KNOWN_SEED_IDS,
  getGoldenLookupStats,
  resetGoldenLookupStats,
} from './adapter/golden-lookup';
