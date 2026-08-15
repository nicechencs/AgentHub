/**
 * mock 不是规则真源。规则真源是 core 的 `ADAPTER_CAPABILITY_MATRIX` /
 * `AdapterRouteService` / `AdapterApplyService`。
 */
import {
  adapterCommandError,
  type AdapterApplyRequest,
  type AdapterApplyResult,
  type AdapterBridgeRuntimeStatus,
  type AdapterPort,
  type AdapterProfile,
  type AdapterProfileFilter,
} from '@/lib/backend/contracts/adapter';
import { delay } from './delay';
import { analyze } from './adapter/analyze';
import { materializeApply } from './adapter/apply';
import { buildPlan, hasCodexAccessToken } from './adapter/plan';
import {
  CODEX_CLAUDE_RULE_ID,
  KIMI_MEMBERSHIP_RULE_IDS,
  hasAccountApiKey,
  isKimiMembershipAccount,
  isKimiMembershipProvider,
  type ClassifiableAccount,
  type MockAdapterSourceResolver,
  type MockAdapterState,
} from './adapter/types';

const adapterStates = new Set<MockAdapterState>();

export function resetMockAdapters(): void {
  adapterStates.forEach((state) => {
    state.generatedProviders.forEach((provider) => state.removeGeneratedProvider?.(provider));
    state.profiles.length = 0;
    state.bridgeStatuses.clear();
    state.generatedProviders.clear();
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

export function createMockAdapterPort(resolver: MockAdapterSourceResolver): AdapterPort {
  const state: MockAdapterState = {
    profiles: [],
    bridgeStatuses: new Map(),
    generatedProviders: new Map(),
    removeGeneratedProvider: resolver.removeGeneratedProvider,
  };
  adapterStates.add(state);

  return {
    async analyze(request) {
      await delay(20);
      return analyze(resolver, request);
    },
    async plan(request) {
      await delay(20);
      return buildPlan(resolver, request, analyze(resolver, request));
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
    async apply(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
      await delay(20);
      const plan = buildPlan(resolver, request, analyze(resolver, request));
      if (!plan.canApply) {
        throw adapterCommandError({
          code: 'unsupported',
          message: '当前适配路径尚不可应用',
          retryable: false,
        });
      }
      // Re-validate source secrets independently of plan.canApply (same rule as core).
      if (plan.analysis.ruleId && KIMI_MEMBERSHIP_RULE_IDS.has(plan.analysis.ruleId)) {
        const providerSource = request.sourceKind === 'provider'
          ? resolver.getProviderById(request.sourceId)
          : undefined;
        const accountSource = request.sourceKind === 'account'
          ? resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined
          : undefined;
        const valid = request.sourceKind === 'provider'
          ? !!providerSource && isKimiMembershipProvider(providerSource)
          : isKimiMembershipAccount(accountSource) && hasAccountApiKey(accountSource);
        if (!valid) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'invalid adapter secret reference',
            retryable: false,
          });
        }
      }
      if (plan.analysis.ruleId === CODEX_CLAUDE_RULE_ID
        && !hasCodexAccessToken(resolver, request.sourceId)) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'invalid adapter secret reference',
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
      };
      return { ...status };
    },
    async setBridgeAutoStart(profileId, autoStart) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      profile.autoStart = autoStart;
      profile.updatedAt = new Date().toISOString();
      return { ...profile };
    },
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
      message: '此适配不需要本地桥接',
      retryable: false,
    });
  }
  return profile;
}

function runningBridgeStatus(profile: AdapterProfile): AdapterBridgeRuntimeStatus {
  const port = profile.localPort ?? 32123;
  return {
    profileId: profile.id,
    state: 'running',
    port,
    endpoint: `http://127.0.0.1:${port}/v1`,
    startedAt: new Date().toISOString(),
    upstreamStatus: 'unknown',
  };
}


export type { MockAdapterSourceResolver } from './adapter/types';
export {
  AGENT_NO_WRITER_REASON,
  CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
  CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
  PROTOCOL_MISMATCH_REASON,
  SAME_PROTOCOL_NO_EDGE_REASON,
} from './adapter/types';
