import { useCallback, useEffect, useRef, useState } from 'react';
import { listAccounts } from '@/lib/api/account';
import { getAdapterBridgeStatus, listAdapterProfiles } from '@/lib/api/adapter';
import { listProviders } from '@/lib/api/provider';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  adapterBridgeProfilesToPoll,
  applyAdapterBridgeStatusPoll,
  loadAdapterPageResources,
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
  const timer = setIntervalFn(() => {
    const pollGeneration = host.getGeneration();
    const current = host.getResources();
    const targets = adapterBridgeProfilesToPoll(current.profiles, current.bridgeStatuses);
    if (targets.length === 0) return;
    void Promise.allSettled(
      targets.map((profile) => getBridgeStatus(profile.id)),
    ).then((results) => {
      if (pollGeneration !== host.getGeneration()) return;
      host.apply((latest) => applyAdapterBridgeStatusPoll(latest, targets, results));
    });
  }, ADAPTER_BRIDGE_STATUS_POLL_MS);
  return () => {
    clearIntervalFn(timer);
  };
}

/** Owns independent resource refreshes and rejects stale responses. */
export function useAdapterResources() {
  const [resources, setResources] = useState<AdapterPageResources>(initialResources);
  const [loading, setLoading] = useState(true);
  const generation = useRef(0);
  const resourcesRef = useRef(resources);
  resourcesRef.current = resources;

  const reload = useCallback(async () => {
    const currentGeneration = ++generation.current;
    setLoading(true);
    const nextResources = await loadAdapterPageResources({
      listAccounts,
      listProviders,
      listProfiles: listAdapterProfiles,
      getBridgeStatus: getAdapterBridgeStatus,
    });
    if (currentGeneration === generation.current) {
      setResources(nextResources);
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const updateBridgeStatus = useCallback((status: AdapterBridgeRuntimeStatus) => {
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
    setResources((current) => ({
      ...current,
      profiles: current.profiles.map((item) => (item.id === profile.id ? profile : item)),
    }));
  }, []);

  const removeProfile = useCallback((profileId: string) => {
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
  }), [reload]);

  return {
    ...resources,
    loading,
    reload,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  };
}
