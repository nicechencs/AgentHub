/**
 * ConnectFlow 来源分组、可行性映射与 OAuth 预检（纯函数）。
 * 行标题 / agentId / isCurrent 等稳定字段来自 `toCredentialRow`（P2-7）。
 */
import { resolveAgentMeta } from '@/config/agents';
import { authDisplayForAccount } from '@/lib/backend/contracts/auth-state';
import { isCapabilityBlocked, providerCapabilityGate } from '@/lib/capability';
import { toCredentialRow } from '@/lib/credential-row';
import type { Account, Provider } from '@/lib/types';
import type { AdapterApplyPlan, AdapterProfile, AdapterRoute } from '@/lib/api/adapter';
import type { AdapterMaturity, AdapterReusePath } from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';
import type {
  PlanEligibility,
  SourceOption,
  SourceOptionsInput,
} from './types';

/** Canonical in this module. */
const INCOMPLETE_OAUTH_HEALTH = new Set(['needs_login', 'missing']);
const INCOMPLETE_OAUTH_STATUS = new Set(['expired', 'none']);

/** Canonical in this module. */
export const OAUTH_INCOMPLETE_MESSAGE = '这份官方登录还没完成，请先完成登录。';

export function oauthIncompleteMessage(t?: TranslateFn): string {
  return t ? t('connect.select.oauthIncomplete') : OAUTH_INCOMPLETE_MESSAGE;
}

/**
 * Native switch reasons come from agent capability gates
 * (`accountSwitch` / `providerCapabilityGate`).
 */
const ACCOUNT_SWITCH_BLOCKED_FALLBACK = '这个工具不能在这里切换账号';
const PROVIDER_SWITCH_BLOCKED_FALLBACK = '这个工具现在不能写入服务配置';

export function accountSwitchBlockedFallback(t?: TranslateFn): string {
  return t ? t('connect.select.accountSwitchBlocked') : ACCOUNT_SWITCH_BLOCKED_FALLBACK;
}

export function providerSwitchBlockedFallback(t?: TranslateFn): string {
  return t ? t('connect.select.providerSwitchBlocked') : PROVIDER_SWITCH_BLOCKED_FALLBACK;
}

const ROUTE_SUMMARY: Record<AdapterRoute, string> = {
  native_endpoint: '',
  local_bridge: '本机路由',
  config_sync: '',
  unsupported: '当前不支持',
};
const REUSE_PATH_SUMMARY: Record<AdapterReusePath, string> = {
  api_endpoint: '',
  native_subscription: '用这份登录',
  local_bridge: '本机路由',
  none: '当前不支持',
};

function reusePathForPlan(plan: AdapterApplyPlan): AdapterReusePath {
  if (plan.reusePath) return plan.reusePath;
  if (plan.analysis.route === 'unsupported') return 'none';
  if (plan.analysis.route === 'local_bridge') return 'local_bridge';
  return 'api_endpoint';
}

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
  t?: TranslateFn,
): SourceOption['state'] {
  if (account.isCurrent) return { kind: 'current' };
  // Capability catalog: isCapabilityBlocked(accountSwitch) + reason.
  if (isCapabilityBlocked(capabilities?.accountSwitch)) {
    return {
      kind: 'blocked_native',
      reason: capabilities?.accountSwitch?.reason ?? accountSwitchBlockedFallback(t),
    };
  }
  return { kind: 'switchable' };
}

function nativeProviderState(
  provider: Provider,
  capabilities: ReturnType<typeof resolveAgentMeta>['capabilities'],
  t?: TranslateFn,
): SourceOption['state'] {
  if (provider.isCurrent) return { kind: 'current' };
  // Capability catalog: providerCapabilityGate(caps).canSwitch + reason.
  const gate = providerCapabilityGate(capabilities, t);
  if (!gate.canSwitch) {
    return {
      kind: 'blocked_native',
      reason: gate.reason ?? providerSwitchBlockedFallback(t),
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

export function planMaturityLabel(maturity: AdapterMaturity | undefined, t?: TranslateFn): string {
  if (maturity === 'stable') return t ? t('connect.select.maturityStable') : '稳定';
  if (maturity === 'experimental') return '';
  if (maturity === 'preview') return t ? t('connect.select.maturityPreview') : '可预览';
  if (maturity === 'none') return '';
  return '';
}

export function planRouteSummary(plan: AdapterApplyPlan, t?: TranslateFn): string {
  const reusePath = reusePathForPlan(plan);
  if (t) {
    if (reusePath === 'local_bridge') return t('kind.route.localRoute');
    if (reusePath === 'native_subscription') return t('kind.route.reuseLogin');
    if (reusePath === 'none') return t('kind.route.unsupported');
    return '';
  }
  return REUSE_PATH_SUMMARY[reusePath] ?? ROUTE_SUMMARY[plan.analysis.route];
}

export function planToEligibility(plan: AdapterApplyPlan): PlanEligibility {
  return {
    kind: 'ready',
    plan,
    canApply: plan.canApply,
    routeSummary: REUSE_PATH_SUMMARY[reusePathForPlan(plan)] ?? ROUTE_SUMMARY[plan.analysis.route],
    ...(plan.canApply ? {} : { reason: plan.reason ?? plan.analysis.reason }),
  };
}

/**
 * Canonical in this module.
 * Applied to Account via the same authDisplay mapping Connections uses.
 */
export function isOauthIncomplete(account: Account): boolean {
  if (account.kind !== 'oauth') return false;
  const display = authDisplayForAccount(account);
  if (display.health && INCOMPLETE_OAUTH_HEALTH.has(display.health)) return true;
  return INCOMPLETE_OAUTH_STATUS.has(display.legacyStatus);
}

export function buildSourceOptions(input: SourceOptionsInput): SourceOption[] {
  const { targetAgentId, accounts, providers, profiles, agentStatuses, t } = input;
  const generatedIds = generatedProviderIds(profiles);
  // Prefer live doctor capabilities when the target status carries them
  // Live doctor capabilities when present; otherwise catalog meta.
  const liveStatus = agentStatuses?.find((status) => status.agentId === targetAgentId);
  const capabilities = liveStatus?.capabilities ?? resolveAgentMeta(targetAgentId).capabilities;
  const native: SourceOption[] = [];
  const cross: SourceOption[] = [];

  const pushNativeAccount = (account: Account) => {
    const row = toCredentialRow({ source: 'account', account });
    native.push({
      ref: { kind: 'account', id: row.id },
      group: 'native',
      agentId: row.agentId,
      label: row.title,
      sublabel: resolveAgentMeta(row.agentId).name,
      state: nativeAccountState(account, capabilities, t),
      account,
    });
  };

  const pushNativeProvider = (provider: Provider) => {
    const row = toCredentialRow({ source: 'provider', provider });
    native.push({
      ref: { kind: 'provider', id: row.id },
      group: 'native',
      agentId: row.agentId,
      label: row.title,
      sublabel: resolveAgentMeta(row.agentId).name,
      state: nativeProviderState(provider, capabilities, t),
      viaAdapter: viaAdapterForProvider(provider, profiles, accounts, providers),
      provider,
    });
  };

  for (const account of accounts) {
    if (account.agentId === targetAgentId) {
      pushNativeAccount(account);
      continue;
    }
    const row = toCredentialRow({ source: 'account', account });
    cross.push({
      ref: { kind: 'account', id: row.id },
      group: 'cross',
      agentId: row.agentId,
      label: row.title,
      sublabel: resolveAgentMeta(row.agentId).name,
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
    // Canonical in this module:
    // generated Providers must not be offered as nested cross-service sources.
    if (isGenerated) continue;
    const row = toCredentialRow({ source: 'provider', provider });
    cross.push({
      ref: { kind: 'provider', id: row.id },
      group: 'cross',
      agentId: row.agentId,
      label: row.title,
      sublabel: resolveAgentMeta(row.agentId).name,
      state: { kind: 'plannable' },
      provider,
    });
  }

  return [...native, ...cross];
}
