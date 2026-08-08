import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/ui-preferences';
import {
  closeToTraySettingValue,
  createTauriSettingsPort,
  resolveCloseToTray,
} from './settings';

const invokeMock = vi.fn();
let tauriRuntime = true;

/** Minimal localStorage for vitest node environment. */
function installMemoryLocalStorage() {
  const store = new Map<string, string>();
  const memory = {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key: string) {
      return store.has(key) ? store.get(key)! : null;
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
    removeItem(key: string) {
      store.delete(key);
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null;
    },
  };
  vi.stubGlobal('localStorage', memory);
  return memory;
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  isTauri: () => tauriRuntime,
  Channel: class {
    onmessage: ((ev: unknown) => void) | null = null;
  },
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: async () => '0.1.0-test',
}));

vi.mock('@/lib/theme', () => ({
  applyTheme: vi.fn(),
}));

describe('resolveCloseToTray', () => {
  it('prefers core over local over default', () => {
    expect(resolveCloseToTray(false, true)).toBe(false);
    expect(resolveCloseToTray(true, false)).toBe(true);
    expect(resolveCloseToTray(undefined, false)).toBe(false);
    expect(resolveCloseToTray(undefined, true)).toBe(true);
    expect(resolveCloseToTray(undefined, undefined)).toBe(true);
    expect(resolveCloseToTray(undefined, undefined, false)).toBe(false);
  });
});

describe('closeToTraySettingValue', () => {
  it('serializes for set_setting', () => {
    expect(closeToTraySettingValue(true)).toBe('true');
    expect(closeToTraySettingValue(false)).toBe('false');
  });
});

describe('createTauriSettingsPort closeToTray', () => {
  const settingsKey = 'agenthub:settings';
  let memory: ReturnType<typeof installMemoryLocalStorage>;

  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    memory = installMemoryLocalStorage();
  });

  afterEach(() => {
    memory.clear();
    vi.unstubAllGlobals();
  });

  it('reads closeToTray from core get_app_settings', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          skillMarketSource: 'auto',
          closeToTray: false,
        };
      }
      if (cmd === 'get_path_info') {
        return {
          dataDir: 'D:/data',
          dbPath: 'D:/data/agenthub.db',
          backupsDir: 'D:/data/backups',
          logsDir: 'D:/data/logs',
        };
      }
      throw new Error(`unexpected invoke: ${cmd}`);
    });

    localStorage.setItem(settingsKey, JSON.stringify({ closeToTray: true }));
    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.closeToTray).toBe(false);
  });

  it('falls back to local when core omits closeToTray', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
        };
      }
      if (cmd === 'get_path_info') {
        return {
          dataDir: 'D:/data',
          dbPath: 'D:/data/agenthub.db',
          backupsDir: 'D:/data/backups',
          logsDir: 'D:/data/logs',
        };
      }
      throw new Error(`unexpected invoke: ${cmd}`);
    });

    localStorage.setItem(settingsKey, JSON.stringify({ closeToTray: false }));
    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.closeToTray).toBe(false);
  });

  it('updateSettings writes close_to_tray via set_setting', async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: { key?: string; value?: string }) => {
      if (cmd === 'set_setting') {
        return;
      }
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: false,
        };
      }
      if (cmd === 'get_path_info') {
        return {
          dataDir: 'D:/data',
          dbPath: 'D:/data/agenthub.db',
          backupsDir: 'D:/data/backups',
          logsDir: 'D:/data/logs',
        };
      }
      throw new Error(`unexpected invoke: ${cmd} ${JSON.stringify(args)}`);
    });

    const port = createTauriSettingsPort();
    await port.updateSettings({ closeToTray: false });

    expect(invokeMock).toHaveBeenCalledWith('set_setting', {
      key: 'close_to_tray',
      value: 'false',
    });

    const stored = JSON.parse(localStorage.getItem(settingsKey) ?? '{}') as {
      closeToTray?: boolean;
    };
    expect(stored.closeToTray).toBe(false);

    // Theme local key is unrelated; ensure we did not require it.
    expect(localStorage.getItem(StorageKey.theme)).toBeNull();
  });
});
