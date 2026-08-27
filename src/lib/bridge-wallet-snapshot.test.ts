import { describe, expect, it } from 'vitest';
import { bridgeWalletSnapshotFromWallet } from './bridge-wallet-snapshot';

describe('bridgeWalletSnapshotFromWallet', () => {
  it('derives binding profile ids and bridge count', () => {
    const snapshot = bridgeWalletSnapshotFromWallet(
      {
        tickets: [],
        bindings: [
          { route: 'bridge', profileId: 'p1', active: true } as never,
          { route: 'native', profileId: 'p2', active: true } as never,
          { route: 'bridge', profileId: undefined, active: true } as never,
        ],
        surfaceGroups: [],
      },
      'ready',
    );
    expect(snapshot.settled).toBe(true);
    expect(snapshot.lastWalletBridgeCount).toBe(2);
    expect([...snapshot.bindingProfileIds]).toEqual(['p1', 'p2']);
  });

  it('marks unsettled while wallet is loading', () => {
    const snapshot = bridgeWalletSnapshotFromWallet(null, 'loading');
    expect(snapshot.settled).toBe(false);
    expect(snapshot.bindingProfileIds.size).toBe(0);
  });
});
