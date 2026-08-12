import { useCallback, useEffect, useRef, useState } from 'react';
import { listAccounts } from '@/lib/api/account';
import { getAdapterBridgeStatus, listAdapterProfiles } from '@/lib/api/adapter';
import { listProviders } from '@/lib/api/provider';
import {
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

/** Owns independent resource refreshes and rejects stale responses. */
export function useAdapterResources() {
  const [resources, setResources] = useState<AdapterPageResources>(initialResources);
  const [loading, setLoading] = useState(true);
  const generation = useRef(0);

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

  return {
    ...resources,
    loading,
    reload,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  };
}
