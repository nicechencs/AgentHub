import { useCallback, useEffect, useRef, useState } from 'react';
import { useConnectionPool } from '@/app/runtime';
import { getAdapterBridgeStatus, listAdapterProfiles } from '@/lib/api/adapter';
import { mergeConnectionEntries } from '@/lib/connection-entry';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  adapterBridgeProfilesToPoll,
  applyAdapterBridgeStatusPoll,
  loadAdapterProfileResources,
  mergeAdapterProfileLoad,
  type AdapterPageResources,
} from './adapter-model';
import type { AdapterBridgeRuntimeStatus } from '@/lib/backend/contracts/adapter';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';

const initialResources: AdapterPageResources = {
  entries: [],
  profiles: [],
  bridgeStatuses: {},
  errors: { bridgeStatuses: {} },
  connectionState: 'ready',
  profileState: 'ready',
};

type AdapterBridgeStatusPollHost = {
  getGeneration: () => number;
  getResources: () => AdapterPageResources;
  apply: (updater: (current: AdapterPageResources) => AdapterPageResources) => void;
  getBridgeStatus?: (profileId: string) => Promise<AdapterBridgeRuntimeStatus>;
  setIntervalFn?: typeof setInterval;
  clearIntervalFn?: typeof clearInterval;
};

/** Starts the 4s stored-status poll and returns a disposer for unmount/generation changes. */
export function startAdapterBridgeStatusPoll(host: AdapterBridgeStatusPollHost): () => void {
  const getBridgeStatus = host.getBridgeStatus ?? getAdapterBridgeStatus;
  const setIntervalFn = host.setIntervalFn ?? setInterval;
  const clearIntervalFn = host.clearIntervalFn ?? clearInterval;
  let disposed = false;
  let pollInFlight = false;
  let pollToken = 0;

  const poll = () => {
    if (disposed || pollInFlight) return;
    const pollGeneration = host.getGeneration();
    const current = host.getResources();
    const targets = adapterBridgeProfilesToPoll(current.profiles, current.bridgeStatuses);
    if (targets.length === 0) return;

    pollInFlight = true;
    const requestToken = ++pollToken;
    void Promise.allSettled(
      targets.map((profile) => getBridgeStatus(profile.id)),
    ).then((results) => {
      if (
        disposed
        || requestToken !== pollToken
        || pollGeneration !== host.getGeneration()
      ) return;
      host.apply((latest) => applyAdapterBridgeStatusPoll(latest, targets, results));
    }).finally(() => {
      if (requestToken === pollToken) pollInFlight = false;
    });
  };

  const timer = setIntervalFn(() => {
    poll();
  }, ADAPTER_BRIDGE_STATUS_POLL_MS);
  return () => {
    disposed = true;
    pollToken += 1;
    clearIntervalFn(timer);
  };
}

function connectionStateFromPool(
  state: ReturnType<typeof useConnectionPool>['state'],
): AdapterPageResources['connectionState'] {
  if (state === 'error') return 'error';
  if (state === 'partial') return 'partial';
  if (state === 'idle' || state === 'loading') return 'loading';
  return 'ready';
}

/** Owns independent resource refreshes and rejects stale responses. */
export function useAdapterResources() {
  const pool = useConnectionPool();
  const [resources, setResources] = useState<AdapterPageResources>(initialResources);
  const [profilesLoading, setProfilesLoading] = useState(true);
  const generation = useRef(0);
  const resourcesRef = useRef(resources);
  resourcesRef.current = resources;

  const reloadProfiles = useCallback(async () => {
    const currentGeneration = ++generation.current;
    setProfilesLoading(true);
    const next = await loadAdapterProfileResources({
      listProfiles: listAdapterProfiles,
      getBridgeStatus: getAdapterBridgeStatus,
    });
    if (currentGeneration !== generation.current) return;
    setResources((current) => {
      const merged = mergeAdapterProfileLoad(current, next);
      return {
        ...current,
        profiles: merged.profiles,
        bridgeStatuses: merged.bridgeStatuses,
        profileState: merged.profileState,
        errors: {
          ...current.errors,
          profiles: merged.profileError,
          bridgeStatuses: merged.bridgeStatusErrors,
        },
      };
    });
    setProfilesLoading(false);
  }, []);

  const reload = useCallback(async () => {
    await Promise.all([pool.reload(), reloadProfiles()]);
  }, [pool.reload, reloadProfiles]);

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  useEffect(() => {
    void reloadProfiles();
  }, [reloadProfiles]);

  useEffect(() => {
    setResources((current) => ({
      ...current,
      entries: mergeConnectionEntries(pool.accounts, pool.providers),
      connectionState: connectionStateFromPool(pool.state),
      errors: {
        ...current.errors,
        accounts: pool.errors.accounts,
        providers: pool.errors.providers,
      },
    }));
  }, [pool.accounts, pool.errors, pool.providers, pool.state]);

  const updateBridgeStatus = useCallback((status: AdapterBridgeRuntimeStatus) => {
    // Start/stop responses are mutations, so invalidate any poll that began
    // before the mutation completed. The mutation result is authoritative.
    generation.current += 1;
    setResources((current) => ({
      ...current,
      bridgeStatuses: {
        ...current.bridgeStatuses,
        [status.profileId]: status,
      },
      errors: {
        ...current.errors,
        bridgeStatuses: Object.fromEntries(
          Object.entries(current.errors.bridgeStatuses).filter(([id]) => id !== status.profileId),
        ),
      },
    }));
  }, []);

  const updateProfile = useCallback((profile: AdapterProfile) => {
    generation.current += 1;
    setResources((current) => ({
      ...current,
      profiles: current.profiles.map((item) => (item.id === profile.id ? profile : item)),
    }));
  }, []);

  const removeProfile = useCallback((profileId: string) => {
    generation.current += 1;
    setResources((current) => ({
      ...current,
      profiles: current.profiles.filter((profile) => profile.id !== profileId),
      bridgeStatuses: Object.fromEntries(
        Object.entries(current.bridgeStatuses).filter(([id]) => id !== profileId),
      ),
    }));
  }, []);

  useEffect(() => startAdapterBridgeStatusPoll({
    getGeneration: () => generation.current,
    getResources: () => resourcesRef.current,
    apply: setResources,
  }), [reloadProfiles]);

  const poolPending = (pool.state === 'idle' || pool.state === 'loading')
    && pool.accounts.length === 0
    && pool.providers.length === 0;

  return {
    ...resources,
    loading: profilesLoading || poolPending,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  };
}
