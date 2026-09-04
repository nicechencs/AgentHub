import { beforeEach, describe, expect, it } from 'vitest';
import { createMockAccountPort, resetMockAccounts, restoreMockAccount } from './account';
import { restoreMockProvider } from './provider';
import { createMockTrashPort, resetMockTrash } from './trash';

describe('mock connection trash', () => {
  beforeEach(() => {
    resetMockTrash();
    resetMockAccounts();
  });

  it('moves a deleted account to trash and restores it as inactive', async () => {
    const accounts = createMockAccountPort();
    const trash = createMockTrashPort({
      restoreAccount: restoreMockAccount,
      restoreProvider: restoreMockProvider,
    });
    const created = await accounts.addApiKeyAccount(
      'claude',
      'sk-test-trash-secret',
      `trash-test-${Date.now()}`,
    );

    await accounts.deleteAccount('claude', created.id);
    const deleted = await trash.list('claude');
    const item = deleted.find((row) => row.sourceId === created.id);
    expect(item?.kind).toBe('account');
    expect((await accounts.listAccounts('claude')).some((row) => row.account.id === created.id)).toBe(false);

    await trash.restore(item!.id);
    const restored = (await accounts.listAccounts('claude')).find((row) => row.account.id === created.id);
    expect(restored?.account.isCurrent).toBe(false);
    expect(await trash.list('claude')).toEqual([]);
  });

  it('restores one deleted login and leaves the others in trash', async () => {
    const accounts = createMockAccountPort();
    const trash = createMockTrashPort({
      restoreAccount: restoreMockAccount,
      restoreProvider: restoreMockProvider,
    });
    const first = await accounts.addApiKeyAccount('claude', 'sk-test-trash-a', 'key-a');
    const second = await accounts.addApiKeyAccount('claude', 'sk-test-trash-b', 'key-b');
    const third = await accounts.addApiKeyAccount('claude', 'sk-test-trash-c', 'key-c');

    await accounts.deleteAccount('claude', first.id);
    await accounts.deleteAccount('claude', second.id);
    await accounts.deleteAccount('claude', third.id);
    const deleted = await trash.list('claude');
    expect(deleted).toHaveLength(3);
    const secondTrash = deleted.find((row) => row.sourceId === second.id);
    expect(secondTrash).toBeDefined();

    await trash.restore(secondTrash!.id);

    const remaining = await trash.list('claude');
    expect(remaining.map((row) => row.sourceId).sort()).toEqual([first.id, third.id].sort());
    const pool = await accounts.listAccounts('claude');
    expect(pool.map((row) => row.account.id).sort()).toEqual([second.id]);
  });
});
