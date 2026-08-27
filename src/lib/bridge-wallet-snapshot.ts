/**
 * Routes page wallet partition inputs derived from the shared ticket wallet.
 */
import type { TicketWallet } from '@/lib/backend/contracts/ticket';

export type BridgeWalletSnapshot = {
  settled: boolean;
  lastWalletBridgeCount: number;
  bindingProfileIds: ReadonlySet<string>;
};

export function bridgeWalletSnapshotFromWallet(
  wallet: TicketWallet | null | undefined,
  loadState: 'idle' | 'loading' | 'ready' | 'error',
): BridgeWalletSnapshot {
  const settled = loadState !== 'idle' && loadState !== 'loading';
  const bindings = wallet?.bindings ?? [];
  return {
    settled,
    lastWalletBridgeCount: bindings.filter((binding) => binding.route === 'bridge').length,
    bindingProfileIds: new Set(
      bindings
        .map((binding) => binding.profileId)
        .filter((id): id is string => typeof id === 'string' && id.length > 0),
    ),
  };
}
