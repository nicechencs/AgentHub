import { afterEach, describe, expect, it } from 'vitest';
import {
  getBridgePresenceSnapshot,
  loadBridgePresence,
  resetBridgePresenceStore,
  shouldShowBridgesNav,
  type BridgePresenceLoaders,
} from './bridge-presence-store';

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
});
