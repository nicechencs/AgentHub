import { afterEach, describe, expect, it } from 'vitest';
import {
  getBridgePresenceSnapshot,
  loadBridgePresence,
  notifyBridgePresenceChanged,
  resetBridgePresenceStore,
  subscribeBridgePresence,
  shouldShowBridgesNav,
  type BridgePresenceLoaders,
} from './bridge-presence-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function loaders(input: {
  profiles?: Array<{ route: 'local_bridge' | 'native_endpoint' }>;
  profileError?: Error;
  walletBridgeCount?: number;
  walletError?: Error;
}): BridgePresenceLoaders {
  return {
    listProfiles: async () => {
      if (input.profileError) throw input.profileError;
      return input.profiles ?? [];
    },
    listWallet: async () => {
      if (input.walletError) throw input.walletError;
      return {
        bindings: Array.from({ length: input.walletBridgeCount ?? 0 }, (_, index) => ({
          ticketId: `t${index}`,
          agentId: 'codex' as const,
          route: 'bridge' as const,
          active: true,
          profileId: `p${index}`,
          bridge: null,
        })),
      };
    },
  };
}

afterEach(() => {
  resetBridgePresenceStore();
});

describe('shouldShowBridgesNav', () => {
  it('hides by default and when both sides are ready empty', () => {
    expect(shouldShowBridgesNav(getBridgePresenceSnapshot())).toBe(false);
    expect(shouldShowBridgesNav({
      status: 'ready',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 0,
      lastNonZero: false,
    })).toBe(false);
  });

  it('shows when a local_bridge profile or wallet bridge exists', () => {
    expect(shouldShowBridgesNav({
      status: 'ready',
      hasLocalBridgeProfile: true,
      walletBridgeCount: 0,
      lastNonZero: true,
    })).toBe(true);
    expect(shouldShowBridgesNav({
      status: 'ready',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 1,
      lastNonZero: true,
    })).toBe(true);
  });

  it('hides a first-load error that was never non-zero', () => {
    expect(shouldShowBridgesNav({
      status: 'error',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 0,
      lastNonZero: false,
    })).toBe(false);
  });

  it('keeps the nav on error after a non-zero session', () => {
    expect(shouldShowBridgesNav({
      status: 'error',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 0,
      lastNonZero: true,
    })).toBe(true);
  });
});

describe('loadBridgePresence', () => {
  it('defaults to hidden until ready, then shows a local_bridge profile including orphans', async () => {
    expect(getBridgePresenceSnapshot().status).toBe('idle');
    expect(shouldShowBridgesNav(getBridgePresenceSnapshot())).toBe(false);
    await loadBridgePresence(loaders({ profiles: [{ route: 'local_bridge' }] }));
    expect(getBridgePresenceSnapshot()).toMatchObject({
      status: 'ready',
      hasLocalBridgeProfile: true,
      walletBridgeCount: 0,
      lastNonZero: true,
    });
    expect(shouldShowBridgesNav(getBridgePresenceSnapshot())).toBe(true);
  });

  it('shows when the wallet still has a bridge binding', async () => {
    await loadBridgePresence(loaders({ walletBridgeCount: 2 }));
    expect(getBridgePresenceSnapshot()).toMatchObject({
      status: 'ready',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 2,
    });
    expect(shouldShowBridgesNav(getBridgePresenceSnapshot())).toBe(true);
  });

  it('keeps last profile/wallet values when a later fetch fails', async () => {
    await loadBridgePresence(loaders({
      profiles: [{ route: 'local_bridge' }],
      walletBridgeCount: 1,
    }));
    await loadBridgePresence(loaders({
      profileError: new Error('profiles down'),
      walletError: new Error('wallet down'),
    }));
    expect(getBridgePresenceSnapshot()).toMatchObject({
      status: 'error',
      hasLocalBridgeProfile: true,
      walletBridgeCount: 1,
      lastNonZero: true,
    });
    expect(shouldShowBridgesNav(getBridgePresenceSnapshot())).toBe(true);
  });

  it('does not treat a failed wallet read as zero bridges', async () => {
    await loadBridgePresence(loaders({ walletBridgeCount: 1 }));
    await loadBridgePresence(loaders({
      profiles: [],
      walletError: new Error('wallet down'),
    }));
    expect(getBridgePresenceSnapshot().walletBridgeCount).toBe(1);
    expect(getBridgePresenceSnapshot().status).toBe('error');
  });

  it('lets the latest overlapping load win', async () => {
    const oldProfiles = deferred<readonly [{ route: 'local_bridge' }]>();
    const oldWallet = deferred<{ bindings: [] }>();
    const oldLoad = loadBridgePresence({
      listProfiles: () => oldProfiles.promise,
      listWallet: () => oldWallet.promise,
    });

    await loadBridgePresence(loaders({ profiles: [], walletBridgeCount: 0 }));
    oldProfiles.resolve([{ route: 'local_bridge' }]);
    oldWallet.resolve({ bindings: [] });
    await oldLoad;

    expect(getBridgePresenceSnapshot()).toMatchObject({
      status: 'ready',
      hasLocalBridgeProfile: false,
      walletBridgeCount: 0,
    });
  });

  it('queues a notification during an in-flight load and refreshes afterward', async () => {
    const initialProfiles = deferred<readonly []>();
    const initialWallet = deferred<{ bindings: [] }>();
    const refreshedProfiles = deferred<readonly [{ route: 'local_bridge' }]>();
    const refreshedWallet = deferred<{ bindings: [] }>();
    const refreshedStarted = deferred<void>();
    const refreshedReady = deferred<void>();
    let refreshedLoaderCount = 0;
    const snapshots: string[] = [];
    const unsubscribe = subscribeBridgePresence(() => {
      const status = getBridgePresenceSnapshot().status;
      snapshots.push(status);
      if (status === 'ready') refreshedReady.resolve(undefined);
    });
    const initialLoad = loadBridgePresence({
      listProfiles: () => initialProfiles.promise,
      listWallet: () => initialWallet.promise,
    });

    notifyBridgePresenceChanged({
      listProfiles: () => {
        if (++refreshedLoaderCount === 2) refreshedStarted.resolve(undefined);
        return refreshedProfiles.promise;
      },
      listWallet: () => {
        if (++refreshedLoaderCount === 2) refreshedStarted.resolve(undefined);
        return refreshedWallet.promise;
      },
    });
    initialProfiles.resolve([]);
    initialWallet.resolve({ bindings: [] });
    await initialLoad;

    // The queued notification invalidates the initial generation before it
    // settles, so its empty result must never be published.
    expect(snapshots).not.toContain('ready');

    // Let the queued rerun attach both loaders. It must not publish a ready
    // snapshot until both of the second request's promises resolve.
    await refreshedStarted.promise;
    expect(snapshots).not.toContain('ready');

    refreshedProfiles.resolve([{ route: 'local_bridge' }]);
    refreshedWallet.resolve({ bindings: [] });
    await refreshedReady.promise;
    expect(getBridgePresenceSnapshot()).toMatchObject({
      status: 'ready',
      hasLocalBridgeProfile: true,
      lastNonZero: true,
    });
    unsubscribe();
  });
});
