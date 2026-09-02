import { mergeConnectionEntries, type ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';
import type {
  Account,
  Provider,
} from '@/lib/types';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';

export type AdapterResourceLoadState = 'loading' | 'ready' | 'partial' | 'error';

export type AdapterResourceErrors = Partial<Record<'accounts' | 'providers' | 'profiles', unknown>> & {
  bridgeStatuses: Record<string, unknown>;
};

export type AdapterPageResources = {
  entries: ConnectionEntry[];
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  errors: AdapterResourceErrors;
  connectionState: AdapterResourceLoadState;
  profileState: Exclude<AdapterResourceLoadState, 'loading' | 'partial'>;
};

export type AdapterResourceLoaders = {
  listAccounts: () => Promise<Account[]>;
  listProviders: () => Promise<Provider[]>;
  listProfiles: () => Promise<AdapterProfile[]>;
  getBridgeStatus: (profileId: string) => Promise<AdapterBridgeRuntimeStatus>;
};

export const ADAPTER_BRIDGE_STATUS_POLL_MS = 4_000;

export function adapterBridgeProfilesToPoll(
  profiles: AdapterProfile[],
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>,
): AdapterProfile[] {
  return profiles.filter((profile) => shouldPollAdapterBridgeStatus(profile, bridgeStatuses[profile.id]));
}

export function applyAdapterBridgeStatusPoll(
  current: AdapterPageResources,
  targets: AdapterProfile[],
  results: PromiseSettledResult<AdapterBridgeRuntimeStatus>[],
): AdapterPageResources {
  const bridgeStatuses = { ...current.bridgeStatuses };
  const bridgeStatusErrors = { ...current.errors.bridgeStatuses };
  results.forEach((result, index) => {
    const profile = targets[index];
    if (!profile) return;
    if (result.status === 'fulfilled') {
      bridgeStatuses[profile.id] = result.value;
      delete bridgeStatusErrors[profile.id];
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatusForPoll(profile, bridgeStatuses[profile.id]);
    bridgeStatusErrors[profile.id] = result.reason;
  });
  return {
    ...current,
    bridgeStatuses,
    errors: { ...current.errors, bridgeStatuses: bridgeStatusErrors },
  };
}

function isFulfilled<T>(result: PromiseSettledResult<T>): result is PromiseFulfilledResult<T> {
  return result.status === 'fulfilled';
}

function unavailableBridgeStatus(profile: AdapterProfile): AdapterBridgeRuntimeStatus {
  return {
    profileId: profile.id,
    state: 'error',
    port: profile.localPort ?? null,
    endpoint: null,
    startedAt: null,
    upstreamStatus: 'unavailable',
    recentInbound: [],
    recentRouteTraces: [],
    totalRequestCount: 0,
    failedRequestCount: 0,
    lastRequestAt: null,
  };
}

/**
 * A later poll/read failure must not invent connectivity or erase the last
 * known port / state / counters. `error` is only a placeholder when the runtime
 * was never observed — it is not a start failure.
 */
export function unavailableBridgeStatusForPoll(
  profile: AdapterProfile,
  previous?: AdapterBridgeRuntimeStatus,
): AdapterBridgeRuntimeStatus {
  return {
    profileId: profile.id,
    state: previous?.state ?? 'error',
    port: previous?.port ?? profile.localPort ?? null,
    endpoint: previous?.endpoint ?? null,
    startedAt: previous?.startedAt ?? null,
    upstreamStatus: 'unavailable',
    recentInbound: previous?.recentInbound ?? [],
    recentRouteTraces: previous?.recentRouteTraces ?? [],
    totalRequestCount: previous?.totalRequestCount ?? 0,
    failedRequestCount: previous?.failedRequestCount ?? 0,
    lastRequestAt: previous?.lastRequestAt ?? null,
    localToken: previous?.localToken ?? null,
  };
}

/**
 * Test helper for independent resource loading. The Bridges page reads
 * connection rows from the shared connection pool via `useAdapterResources`;
 * do not call this from production page code.
 *
 * A failed pool must never erase a successfully loaded pool (in particular,
 * profiles must not look empty when their request failed). Runtime bridge
 * inspection is intentionally best effort: a status failure still returns
 * every persisted profile.
 */
export async function loadAdapterPageResources(
  loaders: AdapterResourceLoaders,
  t?: TranslateFn,
): Promise<AdapterPageResources> {
  const [accountsResult, providersResult, profilesResult] = await Promise.allSettled([
    Promise.resolve().then(loaders.listAccounts),
    Promise.resolve().then(loaders.listProviders),
    Promise.resolve().then(loaders.listProfiles),
  ]);

  const accounts = isFulfilled(accountsResult) ? accountsResult.value : [];
  const providers = isFulfilled(providersResult) ? providersResult.value : [];
  const profiles = isFulfilled(profilesResult) ? profilesResult.value : [];
  const localBridgeProfiles = profiles.filter((profile) => profile.route === 'local_bridge');
  const statusResults = await Promise.allSettled(
    localBridgeProfiles.map((profile) => Promise.resolve().then(() => loaders.getBridgeStatus(profile.id))),
  );

  const bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus> = {};
  const bridgeStatusErrors: Record<string, unknown> = {};
  statusResults.forEach((result, index) => {
    const profile = localBridgeProfiles[index];
    if (isFulfilled(result)) {
      bridgeStatuses[profile.id] = result.value;
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatus(profile);
    bridgeStatusErrors[profile.id] = result.reason;
  });

  const accountError = isFulfilled(accountsResult) ? undefined : accountsResult.reason;
  const providerError = isFulfilled(providersResult) ? undefined : providersResult.reason;
  const profileError = isFulfilled(profilesResult) ? undefined : profilesResult.reason;
  const connectionState = accountError && providerError
    ? 'error'
    : accountError || providerError
      ? 'partial'
      : 'ready';

  return {
    entries: mergeConnectionEntries(accounts, providers, undefined, t),
    profiles,
    bridgeStatuses,
    errors: {
      ...(accountError ? { accounts: accountError } : {}),
      ...(providerError ? { providers: providerError } : {}),
      ...(profileError ? { profiles: profileError } : {}),
      bridgeStatuses: bridgeStatusErrors,
    },
    connectionState,
    profileState: profileError ? 'error' : 'ready',
  };
}

/** Persist profiles only; callers fill bridge status asynchronously. */
export async function loadAdapterProfilesList(
  listProfiles: AdapterResourceLoaders['listProfiles'],
): Promise<{
  profiles: AdapterProfile[];
  profileState: AdapterPageResources['profileState'];
  profileError?: unknown;
}> {
  try {
    const profiles = await Promise.resolve().then(listProfiles);
    return { profiles, profileState: 'ready' };
  } catch (error) {
    return { profiles: [], profileState: 'error', profileError: error };
  }
}

/** Profiles + bridge status only. Connection rows come from the shared pool store. */
export async function loadAdapterProfileResources(
  loaders: Pick<AdapterResourceLoaders, 'listProfiles' | 'getBridgeStatus'>,
): Promise<{
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  profileState: AdapterPageResources['profileState'];
  profileError?: unknown;
  bridgeStatusErrors: Record<string, unknown>;
}> {
  let profiles: AdapterProfile[] = [];
  let profileError: unknown;
  try {
    profiles = await Promise.resolve().then(loaders.listProfiles);
  } catch (error) {
    profileError = error;
  }
  const localBridgeProfiles = profiles.filter((profile) => profile.route === 'local_bridge');
  const statusResults = await Promise.allSettled(
    localBridgeProfiles.map((profile) => Promise.resolve().then(() => loaders.getBridgeStatus(profile.id))),
  );

  const bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus> = {};
  const bridgeStatusErrors: Record<string, unknown> = {};
  statusResults.forEach((result, index) => {
    const profile = localBridgeProfiles[index];
    if (isFulfilled(result)) {
      bridgeStatuses[profile.id] = result.value;
      return;
    }
    bridgeStatuses[profile.id] = unavailableBridgeStatus(profile);
    bridgeStatusErrors[profile.id] = result.reason;
  });

  return {
    profiles,
    bridgeStatuses,
    profileState: profileError ? 'error' : 'ready',
    profileError,
    bridgeStatusErrors,
  };
}

/** Live local-bridge rows that should keep reading stored runtime status. */
export function shouldPollAdapterBridgeStatus(
  profile: Pick<AdapterProfile, 'route'>,
  status?: AdapterBridgeRuntimeStatus,
): boolean {
  if (profile.route !== 'local_bridge') return false;
  return status?.state === 'running' || status?.state === 'degraded';
}

/** Keep the last successful profile list when a later listProfiles call fails. */
export function mergeAdapterProfileLoad(
  previous: {
    profiles: AdapterProfile[];
    bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  },
  next: {
    profiles: AdapterProfile[];
    bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
    profileState: AdapterPageResources['profileState'];
    profileError?: unknown;
    bridgeStatusErrors: Record<string, unknown>;
  },
): {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  profileState: AdapterPageResources['profileState'];
  profileError?: unknown;
  bridgeStatusErrors: Record<string, unknown>;
} {
  if (!next.profileError) return next;
  if (previous.profiles.length === 0) return next;
  return {
    ...next,
    profiles: previous.profiles,
    bridgeStatuses: Object.keys(next.bridgeStatuses).length > 0
      ? next.bridgeStatuses
      : previous.bridgeStatuses,
  };
}

export function resourceFailureMessage(errors: AdapterResourceErrors): string | null {
  const failed = [
    errors.accounts ? '账号' : null,
    errors.providers ? 'API 配置' : null,
  ].filter(Boolean);
  return failed.length ? `部分连接未能加载：${failed.join('、')}。其余仍可用。` : null;
}
