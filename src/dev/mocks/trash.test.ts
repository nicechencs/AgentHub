import { beforeEach, describe, expect, it } from 'vitest';
import { createMockAccountPort, restoreMockAccount } from './account';
import { restoreMockProvider } from './provider';
import { createMockTrashPort, resetMockTrash } from './trash';

describe('mock connection trash', () => {
  beforeEach(() => resetMockTrash());

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
});
