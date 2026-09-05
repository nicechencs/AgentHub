import { describe, expect, it } from 'vitest';
import {
  type PageChromeEntry,
  removePageChromeEntry,
  topPageChrome,
  upsertPageChromeEntry,
} from './page-chrome-model';

describe('page chrome stack', () => {
  it('keeps a hidden page title after the visible page unregisters', () => {
    let entries: PageChromeEntry[] = upsertPageChromeEntry([], 1, { title: 'Sub2API' });
    entries = upsertPageChromeEntry(entries, 2, { title: '连接' });
    expect(topPageChrome(entries)?.title).toBe('连接');

    entries = upsertPageChromeEntry(entries, 1, { title: 'Sub2API', description: '已登录' });
    expect(topPageChrome(entries)?.title).toBe('连接');

    entries = removePageChromeEntry(entries, 2);
    expect(topPageChrome(entries)?.title).toBe('Sub2API');
    expect(topPageChrome(entries)?.description).toBe('已登录');
  });

  it('clears the top bar when the last page unregisters', () => {
    const entries = removePageChromeEntry(
      upsertPageChromeEntry([], 1, { title: 'Sub2API' }),
      1,
    );
    expect(topPageChrome(entries)).toBeNull();
  });
});
