import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Backend, TicketWallet } from '@/lib/backend/contracts';
import {
  getTicketWalletSnapshot,
  loadTicketWallet,
  notifyTicketWalletChanged,
  resetTicketWalletStore,
} from './ticket-wallet-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function wallet(label: string): TicketWallet {
  return {
    tickets: [
      {
        id: `account:${label}`,
        sourceKind: 'account',
        sourceId: label,
        agentId: 'claude',
        label,
        surface: 'claude-subscription',
        credentialClass: 'oauth',
        speaks: ['anthropic_messages'],
        importedFrom: 'claude',
      },
    ],
    bindings: [],
    surfaceGroups: [],
  };
}

function walletBackend(listWallet: ReturnType<typeof vi.fn>): Backend {
  return { ticket: { listWallet } } as unknown as Backend;
}

describe('ticket-wallet-store', () => {
  beforeEach(() => resetTicketWalletStore());

  it('reuses a completed snapshot instead of refetching', async () => {
    const listWallet = vi.fn(async () => wallet('one'));
    const backend = walletBackend(listWallet);

    const first = await loadTicketWallet(backend);
    const second = await loadTicketWallet(backend);

    expect(listWallet).toHaveBeenCalledOnce();
    expect(second).toBe(first);
    expect(second.wallet?.tickets[0]?.label).toBe('one');
  });

  it('deduplicates concurrent loads', async () => {
    const pending = deferred<TicketWallet>();
    const listWallet = vi.fn(() => pending.promise);
    const backend = walletBackend(listWallet);

    const first = loadTicketWallet(backend);
    const second = loadTicketWallet(backend);
    pending.resolve(wallet('shared'));
    const [a, b] = await Promise.all([first, second]);

    expect(listWallet).toHaveBeenCalledOnce();
    expect(a.wallet).toEqual(b.wallet);
  });

  it('force refresh replaces the snapshot', async () => {
    const listWallet = vi
      .fn()
      .mockResolvedValueOnce(wallet('old'))
      .mockResolvedValueOnce(wallet('new'));
    const backend = walletBackend(listWallet);

    await loadTicketWallet(backend);
    const refreshed = await notifyTicketWalletChanged(backend);

    expect(listWallet).toHaveBeenCalledTimes(2);
    expect(refreshed.wallet?.tickets[0]?.label).toBe('new');
    expect(getTicketWalletSnapshot().refreshing).toBe(false);
  });

  it('keeps the last good wallet when a later refresh fails', async () => {
    const listWallet = vi
      .fn()
      .mockResolvedValueOnce(wallet('keep'))
      .mockRejectedValueOnce(new Error('wallet down'));
    const backend = walletBackend(listWallet);

    await loadTicketWallet(backend);
    const failed = await loadTicketWallet(backend, { force: true });

    expect(failed.wallet?.tickets[0]?.label).toBe('keep');
    expect(failed.error).toBeInstanceOf(Error);
    expect(getTicketWalletSnapshot().wallet?.tickets[0]?.label).toBe('keep');
  });
});
