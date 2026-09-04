import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  __resetRememberedAccountsForTests,
  clearAllRememberedAccounts,
  clearAllRememberedPasswords,
  deleteRememberedAccount,
  getLastUsedRememberedAccount,
  getRememberedPassword,
  isSub2ApiRememberEnabled,
  listRememberedAccounts,
  loadRememberedCredentials,
  saveRememberedAccount,
  setSub2ApiRememberEnabled,
} from './remembered-accounts';

describe('sub2api remembered accounts', () => {
  beforeEach(() => {
    const mem = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => mem.get(k) ?? null,
      setItem: (k: string, v: string) => {
        mem.set(k, v);
      },
      removeItem: (k: string) => {
        mem.delete(k);
      },
    });
    __resetRememberedAccountsForTests();
  });

  it('defaults remember toggle to ON', () => {
    expect(isSub2ApiRememberEnabled()).toBe(true);
  });

  it('saves multiple accounts and returns last-used first without passwords in list', () => {
    saveRememberedAccount({
      siteUrl: 'https://a.example/login',
      email: 'one@ex.com',
      password: 'p1',
    });
    saveRememberedAccount({
      siteUrl: 'https://b.example',
      email: 'two@ex.com',
      password: 'p2',
    });
    const list = listRememberedAccounts();
    expect(list).toHaveLength(2);
    expect(list[0]?.email).toBe('two@ex.com');
    expect(list[0]?.siteUrl).toBe('https://b.example');
    expect(list[1]?.siteUrl).toBe('https://a.example');
    expect(JSON.stringify(list)).not.toContain('p1');
    expect(JSON.stringify(list)).not.toContain('p2');
    expect(getLastUsedRememberedAccount()?.email).toBe('two@ex.com');
    const creds = loadRememberedCredentials(list[0]!.id);
    expect(creds?.password).toBe('p2');
  });

  it('does not save when toggle is OFF', () => {
    setSub2ApiRememberEnabled(false);
    expect(saveRememberedAccount({
      siteUrl: 'https://a.example',
      email: 'one@ex.com',
      password: 'secret',
    })).toBeNull();
    expect(listRememberedAccounts()).toHaveLength(0);
  });

  it('deletes one account and can clear passwords only', () => {
    const a = saveRememberedAccount({
      siteUrl: 'https://a.example',
      email: 'one@ex.com',
      password: 'p1',
    })!;
    const b = saveRememberedAccount({
      siteUrl: 'https://b.example',
      email: 'two@ex.com',
      password: 'p2',
    })!;
    deleteRememberedAccount(a.id);
    expect(listRememberedAccounts().map((r) => r.id)).toEqual([b.id]);
    clearAllRememberedPasswords();
    expect(listRememberedAccounts()).toHaveLength(1);
    expect(getRememberedPassword(b.id)).toBeNull();
    expect(loadRememberedCredentials(b.id)?.password).toBe('');
    clearAllRememberedAccounts();
    expect(listRememberedAccounts()).toHaveLength(0);
    expect(localStorage.getItem(StorageKey.sub2apiRememberedSecrets)).toBeNull();
  });
});
