import { describe, expect, it } from 'vitest';
import { flattenKeys, translate } from '../index';
import { en } from './en';
import { zh } from './zh';

const BANNED = /票|钱包|投影|协议桥|PKCE|loopback|[①②③]|实验|未验证/;

function lookup(obj: unknown, key: string): string {
  let cur: unknown = obj;
  for (const part of key.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return key;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : key;
}

describe('locale key parity', () => {
  it('zh and en expose the same leaf keys', () => {
    const zhKeys = flattenKeys(zh).sort();
    const enKeys = flattenKeys(en).sort();
    expect(enKeys).toEqual(zhKeys);
    expect(zhKeys.length).toBeGreaterThan(50);
  });

  it('covers dashboard / connections / connect / chat / agents / projects / mcp namespaces', () => {
    const keys = flattenKeys(zh);
    expect(keys.some((k) => k.startsWith('dashboard.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('connections.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('connect.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('chat.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('agents.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('projects.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('mcp.'))).toBe(true);
    expect(keys.some((k) => k.startsWith('connections.providerDialog.'))).toBe(true);
    expect(keys).toContain('dashboard.sync.manualOnly');
    expect(keys).toContain('connect.select.oauthIncomplete');
    expect(keys).toContain('kind.route.localRoute');
    expect(keys).toContain('chat.page.emptyTitle');
    expect(keys).toContain('agents.page.title');
    expect(keys).toContain('projects.page.title');
    expect(keys).toContain('mcp.page.title');
    expect(keys).toContain('connections.providerDialog.useOfficial');
  });

  it('dashboard / connections / connect / chat / agents / projects / mcp user copy avoids banned jargon', () => {
    const keys = flattenKeys(zh).filter((k) =>
      k.startsWith('dashboard.')
      || k.startsWith('connections.')
      || k.startsWith('connect.')
      || k.startsWith('chat.')
      || k.startsWith('agents.')
      || k.startsWith('projects.')
      || k.startsWith('mcp.'),
    );
    expect(keys.length).toBeGreaterThan(20);
    for (const key of keys) {
      expect(lookup(zh, key), key).not.toMatch(BANNED);
      expect(lookup(en, key), key).not.toMatch(BANNED);
    }
  });

  it('translates dashboard sync and connect select in both languages', () => {
    expect(translate('zh', 'dashboard.sync.manualOnly')).toBe('仅手动采集');
    expect(translate('en', 'dashboard.sync.manualOnly')).toBe('Manual collect only');
    expect(translate('zh', 'connect.select.maturityStable')).toBe('稳定');
    expect(translate('en', 'connect.select.maturityStable')).toBe('Stable');
    expect(translate('en', 'kind.route.localRoute')).toBe('Local route');
  });
});
