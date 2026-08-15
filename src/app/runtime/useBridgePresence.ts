import { useSyncExternalStore } from 'react';
import {
  getBridgePresenceSnapshot,
  subscribeBridgePresence,
  type BridgePresenceSnapshot,
} from './bridge-presence-store';

export function useBridgePresence(): BridgePresenceSnapshot {
  return useSyncExternalStore(
    subscribeBridgePresence,
    getBridgePresenceSnapshot,
    getBridgePresenceSnapshot,
  );
}
