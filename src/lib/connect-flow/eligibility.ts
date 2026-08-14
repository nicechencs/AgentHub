/**
 * ConnectFlow 来源分组、可行性映射与 OAuth 预检（纯函数）。
 */
import { resolveAgentMeta } from '@/config/agents';
import { authDisplayForAccount } from '@/lib/backend/contracts/auth-state';
import { isCapabilityBlocked, providerCapabilityGate } from '@/lib/capability';
import type { Account, Provider } from '@/lib/types';
import type { AdapterApplyPlan, AdapterProfile, AdapterRoute } from '@/lib/api/adapter';
import type {
  PlanEligibility,
  SourceOption,
  SourceOptionsInput,
} from './types';

/** Copied from src/pages/adapter/adapter-sources.ts (isOAuthAuthIncomplete). */
const INCOMPLETE_OAUTH_HEALTH = new Set(['needs_login', 'missing']);
const INCOMPLETE_OAUTH_STATUS = new Set(['expired', 'none']);

/** Copied from src/pages/adapter/adapter-sources.ts (oauthIncompleteAuthHint). */
export const OAUTH_INCOMPLETE_MESSAGE = '官方登录未完成，先到 Connections 授权。';

/**
 * Copied from src/pages/connections/ConnectionList.tsx (~183-200, 328-364)
 * and ConnectionCard.tsx fallback. Phase 2 should unify with Connections.
 */
const ACCOUNT_SWITCH_BLOCKED_FALLBACK = '该 Agent 不支持账号池切换';
const PROVIDER_SWITCH_BLOCKED_FALLBACK = '当前 Agent 不支持 Provider 配置写入';

const ROUTE_SUMMARY: Record<AdapterRoute, string> = {
  native_endpoint: '直连端点映射',
  local_bridge: '本地桥',
  config_sync: '直接同步',
  unsupported: '当前不支持',
};

function generatedProviderIds(profiles: readonly Pick<AdapterProfile, 'generatedProviderId'>[]): Set<string> {
  return new Set(
    profiles
      .map((profile) => profile.generatedProviderId)
      .filter((id): id is string => typeof id === 'string' && id.length > 0),
  );
}

function sourceDisplayName(
  sourceKind: 'account' | 'provider',
  sourceId: string,
  accounts: readonly Account[],
  providers: readonly Provider[],
): string {
  if (sourceKind === 'account') {
    return accounts.find((account) => account.id === sourceId)?.label ?? sourceId;
  }
  return providers.find((provider) => provider.id === sourceId)?.name ?? sourceId;
}

function nativeAccountState(
  account: Account,
  capabilities: ReturnType<typeof resolveAgentMeta>['capabilities'],
): SourceOption['state'] {
  if (account.isCurrent) return { kind: 'current' };
  // Copied from ConnectionList.tsx: isCapabilityBlocked(accountSwitch) + reason.
  if (isCapabilityBlocked(capabilities?.accountSwitch)) {
    return {
      kind: 'blocked_native',
      reason: capabilities?.accountSwitch?.reason ?? ACCOUNT_SWITCH_BLOCKED_FALLBACK,
    };
  }
  return { kind: 'switchable' };
}

function nativeProviderState(
  provider: Provider,
  capabilities: ReturnType<typeof resolveAgentMeta>['capabilities'],
): SourceOption['state'] {
  if (provider.isCurrent) return { kind: 'current' };
  // Copied from ConnectionList.tsx: providerCapabilityGate(caps).canSwitch + reason.
  const gate = providerCapabilityGate(capabilities);
  if (!gate.canSwitch) {
    return {
      kind: 'blocked_native',
      reason: gate.reason ?? PROVIDER_SWITCH_BLOCKED_FALLBACK,
    };
  }
  return { kind: 'switchable' };
}

function viaAdapterForProvider(
  provider: Provider,
  profiles: readonly AdapterProfile[],
  accounts: readonly Account[],
  providers: readonly Provider[],
): SourceOption['viaAdapter'] | undefined {
  const profile = profiles.find((item) => item.generatedProviderId === provider.id);
  if (!profile) return undefined;
  return {
    sourceLabel: sourceDisplayName(profile.sourceKind, profile.sourceId, accounts, providers),
  };
}

export function planToEligibility(plan: AdapterApplyPlan): PlanEligibility {
  return {
    kind: 'ready',
    plan,
    canApply: plan.canApply,
    routeSummary: ROUTE_SUMMARY[plan.analysis.route],
    ...(plan.canApply ? {} : { reason: plan.analysis.reason }),
  };
}

/**
 * Copied from src/pages/adapter/adapter-sources.ts (isOAuthAuthIncomplete),
 * applied to Account via the same authDisplay mapping Connections uses.
 */
export function isOauthIncomplete(account: Account): boolean {
  if (account.kind !== 'oauth') return false;
  const display = authDisplayForAccount(account);
  if (display.health && INCOMPLETE_OAUTH_HEALTH.has(display.health)) return true;
  return INCOMPLETE_OAUTH_STATUS.has(display.legacyStatus);
}

export function buildSourceOptions(input: SourceOptionsInput): SourceOption[] {
  const { targetAgentId, accounts, providers, profiles } = input;
  const generatedIds = generatedProviderIds(profiles);
  // SourceOptionsInput has no live agentStatuses; catalog capabilities are the
  // same fallback ConnectionList uses for providers (meta?.capabilities).
  const capabilities = resolveAgentMeta(targetAgentId).capabilities;
  const native: SourceOption[] = [];
  const cross: SourceOption[] = [];

  const pushNativeAccount = (account: Account) => {
    native.push({
      ref: { kind: 'account', id: account.id },
      group: 'native',
      agentId: account.agentId,
      label: account.label,
      sublabel: resolveAgentMeta(account.agentId).name,
      state: nativeAccountState(account, capabilities),
      account,
    });
  };

  const pushNativeProvider = (provider: Provider) => {
    native.push({
      ref: { kind: 'provider', id: provider.id },
      group: 'native',
      agentId: provider.agentId,
      label: provider.name,
      sublabel: resolveAgentMeta(provider.agentId).name,
      state: nativeProviderState(provider, capabilities),
      viaAdapter: viaAdapterForProvider(provider, profiles, accounts, providers),
      provider,
    });
  };

  for (const account of accounts) {
    if (account.agentId === targetAgentId) {
      pushNativeAccount(account);
      continue;
    }
    cross.push({
      ref: { kind: 'account', id: account.id },
      group: 'cross',
      agentId: account.agentId,
      label: account.label,
      sublabel: resolveAgentMeta(account.agentId).name,
      state: { kind: 'plannable' },
      account,
    });
  }

  for (const provider of providers) {
    const isGenerated = generatedIds.has(provider.id);
    if (provider.agentId === targetAgentId) {
      pushNativeProvider(provider);
      continue;
    }
    // Copied from adapter-sources.ts excludeAdapterGeneratedSources:
    // generated Providers must not be offered as nested cross-service sources.
    if (isGenerated) continue;
    cross.push({
      ref: { kind: 'provider', id: provider.id },
      group: 'cross',
      agentId: provider.agentId,
      label: provider.name,
      sublabel: resolveAgentMeta(provider.agentId).name,
      state: { kind: 'plannable' },
      provider,
    });
  }

  return [...native, ...cross];
}
