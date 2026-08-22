/**
 * Sidebar visibility for Bridges. Does not partition bound/orphan, does not
 * subscribe to the connection pool, and does not poll health.
 */
import { getBackend } from './backend-runtime';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';

export type BridgePresenceStatus = 'idle' | 'loading' | 'ready' | 'error';

export type BridgePresenceSnapshot = {
  /** Profiles and wallet each keep last success; either side failing is error. */
  status: BridgePresenceStatus;
  /** Last successful listProfiles had any route=local_bridge (including orphans). */
  hasLocalBridgeProfile: boolean;
  /** Last successful wallet.bindings with route=bridge; failure must not write 0. */
  walletBridgeCount: number;
  /** This session was once ready and (hasLocalBridgeProfile ∨ walletBridgeCount>0). */
  lastNonZero: boolean;
};

type Listener = () => void;

const initialSnapshot: BridgePresenceSnapshot = {
  status: 'idle',
  hasLocalBridgeProfile: false,
  walletBridgeCount: 0,
  lastNonZero: false,
};

let snapshot: BridgePresenceSnapshot = { ...initialSnapshot };
const listeners = new Set<Listener>();
let inflight: Promise<void> | null = null;
let loadGeneration = 0;
let pendingRerun = false;
let pendingLoaders: BridgePresenceLoaders | undefined;

function emit(): void {
  for (const listener of listeners) listener();
}

function setSnapshot(next: BridgePresenceSnapshot): void {
  snapshot = next;
  emit();
}

export function getBridgePresenceSnapshot(): BridgePresenceSnapshot {
  return snapshot;
}

export function subscribeBridgePresence(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function shouldShowBridgesNav(s: BridgePresenceSnapshot): boolean {
  if (s.hasLocalBridgeProfile) return true;
  if (s.walletBridgeCount > 0) return true;
  if (s.status === 'error' && s.lastNonZero) return true;
  return false;
}

export type BridgePresenceLoaders = {
  listProfiles: () => Promise<readonly Pick<AdapterProfile, 'route'>[]>;
  listWallet: () => Promise<Pick<TicketWallet, 'bindings'>>;
};

function defaultLoaders(): BridgePresenceLoaders {
  const backend = getBackend();
  return {
    listProfiles: () => backend.adapter.listProfiles(),
    listWallet: () => backend.ticket.listWallet(),
  };
}

export async function loadBridgePresence(loaders: BridgePresenceLoaders = defaultLoaders()): Promise<void> {
  const requestGeneration = ++loadGeneration;
  if (snapshot.status === 'idle') {
    setSnapshot({ ...snapshot, status: 'loading' });
  }

  const request = (async () => {
    const [profilesResult, walletResult] = await Promise.allSettled([
      Promise.resolve().then(loaders.listProfiles),
      Promise.resolve().then(loaders.listWallet),
    ]);

    // A newer load (for example, one queued after bind/unbind) owns the
    // snapshot. Older responses may settle, but must not roll it back.
    if (requestGeneration !== loadGeneration) return;

    let { hasLocalBridgeProfile, walletBridgeCount } = snapshot;
    let failed = false;

    if (profilesResult.status === 'fulfilled') {
      hasLocalBridgeProfile = profilesResult.value.some((profile) => profile.route === 'local_bridge');
    } else {
      failed = true;
    }

    if (walletResult.status === 'fulfilled') {
      walletBridgeCount = walletResult.value.bindings.filter((binding) => binding.route === 'bridge').length;
    } else {
      failed = true;
    }

    const lastNonZero = snapshot.lastNonZero || hasLocalBridgeProfile || walletBridgeCount > 0;
    setSnapshot({
      status: failed ? 'error' : 'ready',
      hasLocalBridgeProfile,
      walletBridgeCount,
      lastNonZero,
    });
  })();

  inflight = request;
  await request;
  if (inflight === request) {
    inflight = null;
    if (pendingRerun) {
      const rerunLoaders = pendingLoaders;
      pendingRerun = false;
      pendingLoaders = undefined;
      void loadBridgePresence(rerunLoaders);
    }
  }
}

/** Re-read profiles + wallet after bind/unbind. */
export function notifyBridgePresenceChanged(loaders?: BridgePresenceLoaders): void {
  if (inflight) {
    // Invalidate the in-flight response immediately. Otherwise the old load
    // can publish a stale ready/error snapshot before the queued refresh
    // starts.
    loadGeneration += 1;
    pendingRerun = true;
    pendingLoaders = loaders ?? defaultLoaders();
    return;
  }
  void loadBridgePresence(loaders ?? defaultLoaders());
}

/** Test-only. */
export function resetBridgePresenceStore(): void {
  loadGeneration += 1;
  inflight = null;
  pendingRerun = false;
  pendingLoaders = undefined;
  setSnapshot({ ...initialSnapshot });
}
