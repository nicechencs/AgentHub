import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  applyWorkspacePins,
  loadChatWorkspacePins,
  parseWorkspacePinKeys,
  saveChatWorkspacePins,
  toggleWorkspacePinKeys,
} from './chat-workspace-pins';

const store = new Map<string, string>();

beforeEach(() => {
  store.clear();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('workspace pin keys', () => {
  it('keeps unique trimmed keys in order', () => {
    expect(parseWorkspacePinKeys([' path:a ', 'path:b', 'path:a', '', 1, null])).toEqual([
      'path:a',
      'path:b',
    ]);
    expect(parseWorkspacePinKeys({ keys: ['path:a'] })).toEqual([]);
  });

  it('pins a folder to the front and unpins it in place', () => {
    expect(toggleWorkspacePinKeys(['path:old'], 'path:new')).toEqual(['path:new', 'path:old']);
    expect(toggleWorkspacePinKeys(['path:new', 'path:old'], 'path:new')).toEqual(['path:old']);
    expect(toggleWorkspacePinKeys(['path:old'], '  ')).toEqual(['path:old']);
  });

  it('lifts pinned folders above recency order', () => {
    const groups = [
      { key: 'path:newer', label: 'newer' },
      { key: 'path:older', label: 'older' },
      { key: 'unset', label: 'unset' },
    ];
    expect(applyWorkspacePins(groups, ['unset', 'path:older']).map((group) => group.key)).toEqual([
      'unset',
      'path:older',
      'path:newer',
    ]);
    expect(applyWorkspacePins(groups, []).map((group) => group.key)).toEqual([
      'path:newer',
      'path:older',
      'unset',
    ]);
  });

  it('persists pins on the chat workspace key', () => {
    saveChatWorkspacePins([' path:a ', 'path:a', 'path:b']);
    expect(store.get(StorageKey.chatWorkspacePins)).toBe(JSON.stringify(['path:a', 'path:b']));
    expect(loadChatWorkspacePins()).toEqual(['path:a', 'path:b']);
  });
});
