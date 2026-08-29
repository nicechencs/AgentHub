import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useConnectionPool, useTicketWallet } from '@/app/runtime';
import { useI18n } from '@/components/shared/LanguageProvider';
import { getAdapterBridgeStatus, listAdapterProfiles } from '@/lib/api/adapter';
import { bridgeWalletSnapshotFromWallet } from '@/lib/bridge-wallet-snapshot';
import { mergeConnectionEntries } from '@/lib/connection-entry';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  adapterBridgeProfilesToPoll,
  applyAdapterBridgeStatusPoll,
  loadAdapterProfilesList,
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
  const { t } = useI18n();
  const pool = useConnectionPool();
  const ticketWallet = useTicketWallet();
  const [resources, setResources] = useState<AdapterPageResources>(initialResources);
  const [profilesLoading, setProfilesLoading] = useState(true);
  const generation = useRef(0);
  const resourcesRef = useRef(resources);
  resourcesRef.current = resources;

  const reloadProfiles = useCallback(async () => {
    const currentGeneration = ++generation.current;
    setProfilesLoading(true);
    const listed = await loadAdapterProfilesList(listAdapterProfiles);
    if (currentGeneration !== generation.current) return;
    // Paint persisted profiles before per-profile bridge inspection.
    setResources((current) => {
      const merged = mergeAdapterProfileLoad(current, {
        profiles: listed.profiles,
        bridgeStatuses: {},
        profileState: listed.profileState,
        profileError: listed.profileError,
        bridgeStatusErrors: {},
      });
      return {
        ...current,
        profiles: merged.profiles,
        profileState: merged.profileState,
        errors: {
          ...current.errors,
          profiles: merged.profileError,
          bridgeStatuses: current.errors.bridgeStatuses,
        },
      };
    });
    setProfilesLoading(false);

    const localBridgeProfiles = listed.profiles.filter((profile) => profile.route === 'local_bridge');
    if (listed.profileError || localBridgeProfiles.length === 0) return;
    const statusResults = await Promise.allSettled(
      localBridgeProfiles.map((profile) => getAdapterBridgeStatus(profile.id)),
    );
    if (currentGeneration !== generation.current) return;
    setResources((current) => applyAdapterBridgeStatusPoll(current, localBridgeProfiles, statusResults));
  }, []);

  const reload = useCallback(async () => {
    await Promise.all([pool.reload(), reloadProfiles()]);
  }, [pool.reload, reloadProfiles]);

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  useEffect(() => {
    if (ticketWallet.state === 'idle') void ticketWallet.ensureLoaded();
  }, [ticketWallet.ensureLoaded, ticketWallet.state]);

  const wallet = useMemo(
    () => bridgeWalletSnapshotFromWallet(ticketWallet.wallet, ticketWallet.state),
    [ticketWallet.state, ticketWallet.wallet],
  );

  useEffect(() => {
    void reloadProfiles();
  }, [reloadProfiles]);

  useEffect(() => {
    setResources((current) => ({
      ...current,
      entries: mergeConnectionEntries(pool.accounts, pool.providers, undefined, t),
      connectionState: connectionStateFromPool(pool.state),
      errors: {
        ...current.errors,
        accounts: pool.errors.accounts,
        providers: pool.errors.providers,
      },
    }));
  }, [pool.accounts, pool.errors, pool.providers, pool.state, t]);

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
    wallet,
    loading: profilesLoading || poolPending,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    updateProfile,
    removeProfile,
  };
}
