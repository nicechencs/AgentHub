import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  __resetRememberedSitesForTests,
  deleteRememberedSite,
  listRememberedSites,
  saveRememberedSite,
  seedRememberedSitesIfUnset,
} from './remembered-sites';

describe('sub2api remembered sites', () => {
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
    __resetRememberedSitesForTests();
  });

  it('saves unique normalized sites with last-used first', () => {
    expect(saveRememberedSite('https://a.example/login')).toBe('https://a.example');
    expect(saveRememberedSite('b.example')).toBe('https://b.example');
    expect(saveRememberedSite('https://a.example/')).toBe('https://a.example');
    expect(listRememberedSites()).toEqual(['https://a.example', 'https://b.example']);
  });

  it('rejects empty or invalid URLs', () => {
    expect(saveRememberedSite('')).toBeNull();
    expect(saveRememberedSite('not a url')).toBeNull();
    expect(listRememberedSites()).toEqual([]);
  });

  it('deletes one site and keeps the rest', () => {
    saveRememberedSite('https://a.example');
    saveRememberedSite('https://b.example');
    deleteRememberedSite('https://a.example/login');
    expect(listRememberedSites()).toEqual(['https://b.example']);
  });

  it('seeds only when the sites key has never been written', () => {
    seedRememberedSitesIfUnset(['https://a.example', 'https://b.example/login']);
    expect(listRememberedSites()).toEqual(['https://b.example', 'https://a.example']);
    deleteRememberedSite('https://a.example');
    deleteRememberedSite('https://b.example');
    expect(listRememberedSites()).toEqual([]);
    seedRememberedSitesIfUnset(['https://c.example']);
    expect(listRememberedSites()).toEqual([]);
    expect(localStorage.getItem(StorageKey.sub2apiRememberedSites)).not.toBeNull();
  });
});
