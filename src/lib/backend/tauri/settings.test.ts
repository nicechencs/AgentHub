import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/ui-preferences';
import {
  closeToTraySettingValue,
  createTauriSettingsPort,
  isAlreadyDisabledAutostartError,
  resolveCloseToTray,
  resolveUsageCollectIntervalMin,
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

const autostartIsEnabled = vi.fn(async () => false);
const autostartEnable = vi.fn(async () => {});
const autostartDisable = vi.fn(async () => {});

vi.mock('@tauri-apps/plugin-autostart', () => ({
  isEnabled: () => autostartIsEnabled(),
  enable: () => autostartEnable(),
  disable: () => autostartDisable(),
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
    autostartIsEnabled.mockReset().mockResolvedValue(false);
    autostartEnable.mockReset().mockResolvedValue(undefined);
    autostartDisable.mockReset().mockResolvedValue(undefined);
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

    // closeToTray patch must not write theme to core; cache may sync from getSettings.
    const themeSets = invokeMock.mock.calls.filter(
      (c) => c[0] === 'set_setting' && (c[1] as { key?: string } | undefined)?.key === 'theme',
    );
    expect(themeSets).toHaveLength(0);
    expect(localStorage.getItem(StorageKey.theme)).toBe('system');
  });

  it('reads autoStart from OS login item when plugin is available', async () => {
    autostartIsEnabled.mockResolvedValue(true);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
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

    localStorage.setItem(settingsKey, JSON.stringify({ autoStart: false }));
    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.autoStart).toBe(true);
  });

  it('updateSettings writes OS autostart enable/disable', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_setting') return;
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
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

    const port = createTauriSettingsPort();
    // Plugin reports currently disabled → enable should call enable().
    autostartIsEnabled.mockResolvedValue(false);
    await port.updateSettings({ autoStart: true });
    expect(autostartEnable).toHaveBeenCalledTimes(1);

    // Plugin reports currently enabled → disable should call disable().
    autostartIsEnabled.mockResolvedValue(true);
    await port.updateSettings({ autoStart: false });
    expect(autostartDisable).toHaveBeenCalledTimes(1);
  });

  it('updateSettings skips OS autostart write when already at desired state', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_setting') return;
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
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

    autostartIsEnabled.mockResolvedValue(false);
    const port = createTauriSettingsPort();
    await port.updateSettings({ autoStart: false });
    expect(autostartDisable).not.toHaveBeenCalled();
    expect(autostartEnable).not.toHaveBeenCalled();
  });

  it('updateSettings treats missing Run key on disable as success', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_setting') return;
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
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

    // isEnabled may report true while the Run value is already gone, or probe fails.
    autostartIsEnabled.mockResolvedValue(true);
    autostartDisable.mockRejectedValueOnce(
      new Error('系统找不到指定的文件。 (os error 2)'),
    );

    const port = createTauriSettingsPort();
    await expect(port.updateSettings({ autoStart: false })).resolves.toBeDefined();
    expect(autostartDisable).toHaveBeenCalledTimes(1);
  });
});

describe('createTauriSettingsPort leftover keys', () => {
  const settingsKey = 'agenthub:settings';
  let memory: ReturnType<typeof installMemoryLocalStorage>;

  function stubCoreSettings() {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_setting') return;
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
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
  }

  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    autostartIsEnabled.mockReset().mockResolvedValue(false);
    autostartEnable.mockReset().mockResolvedValue(undefined);
    autostartDisable.mockReset().mockResolvedValue(undefined);
    memory = installMemoryLocalStorage();
    stubCoreSettings();
  });

  afterEach(() => {
    memory.clear();
    vi.unstubAllGlobals();
  });

  it('ignores leftover localStorage keys from removed settings fields', async () => {
    localStorage.setItem(
      settingsKey,
      JSON.stringify({
        hasMasterPassword: true,
        credentialStore: 'encrypted-file',
        autoBackup: false,
        usageCollectIntervalMin: 15,
      }),
    );

    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.usageCollectIntervalMin).toBe(15);
    expect(s).not.toHaveProperty('hasMasterPassword');
    expect(s).not.toHaveProperty('credentialStore');
    expect(s).not.toHaveProperty('autoBackup');
  });

  it('does not write removed settings keys back to localStorage', async () => {
    localStorage.setItem(
      settingsKey,
      JSON.stringify({
        hasMasterPassword: true,
        credentialStore: 'keyring',
        autoBackup: false,
        usageCollectIntervalMin: 15,
      }),
    );

    const port = createTauriSettingsPort();
    await port.updateSettings({ usageCollectIntervalMin: 20 });

    const stored = JSON.parse(localStorage.getItem(settingsKey) ?? '{}') as Record<
      string,
      unknown
    >;
    expect(stored.usageCollectIntervalMin).toBe(20);
    expect(stored).not.toHaveProperty('hasMasterPassword');
    expect(stored).not.toHaveProperty('credentialStore');
    expect(stored).not.toHaveProperty('autoBackup');
  });
});

describe('resolveUsageCollectIntervalMin', () => {
  it('prefers core over local over default', () => {
    expect(resolveUsageCollectIntervalMin(45, 15)).toBe(45);
    expect(resolveUsageCollectIntervalMin(0, 15)).toBe(0);
    expect(resolveUsageCollectIntervalMin(undefined, 15)).toBe(15);
    expect(resolveUsageCollectIntervalMin(undefined, undefined)).toBe(30);
  });
});

describe('createTauriSettingsPort usage interval and theme', () => {
  const settingsKey = 'agenthub:settings';
  let memory: ReturnType<typeof installMemoryLocalStorage>;

  function stubCore(overrides: Record<string, unknown> = {}) {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'set_setting') return;
      if (cmd === 'get_app_settings') {
        return {
          theme: 'system',
          language: 'zh-CN',
          logLevel: 'info',
          logRetentionDays: 14,
          closeToTray: true,
          ...overrides,
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
  }

  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
    autostartIsEnabled.mockReset().mockResolvedValue(false);
    autostartEnable.mockReset().mockResolvedValue(undefined);
    autostartDisable.mockReset().mockResolvedValue(undefined);
    memory = installMemoryLocalStorage();
  });

  afterEach(() => {
    memory.clear();
    vi.unstubAllGlobals();
  });

  it('reads usageCollectIntervalMin from core over local', async () => {
    stubCore({ usageCollectIntervalMin: 45 });
    localStorage.setItem(settingsKey, JSON.stringify({ usageCollectIntervalMin: 15 }));

    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.usageCollectIntervalMin).toBe(45);

    const intervalSets = invokeMock.mock.calls.filter(
      (c) =>
        c[0] === 'set_setting' &&
        (c[1] as { key?: string } | undefined)?.key === 'usage_collect_interval_min',
    );
    expect(intervalSets).toHaveLength(0);
  });

  it('treats core interval 0 as authoritative (manual only)', async () => {
    stubCore({ usageCollectIntervalMin: 0 });
    localStorage.setItem(settingsKey, JSON.stringify({ usageCollectIntervalMin: 30 }));

    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.usageCollectIntervalMin).toBe(0);
  });

  it('writes usageCollectIntervalMin via set_setting', async () => {
    stubCore({ usageCollectIntervalMin: 20 });
    const port = createTauriSettingsPort();
    await port.updateSettings({ usageCollectIntervalMin: 60 });

    expect(invokeMock).toHaveBeenCalledWith('set_setting', {
      key: 'usage_collect_interval_min',
      value: '60',
    });

    const stored = JSON.parse(localStorage.getItem(settingsKey) ?? '{}') as {
      usageCollectIntervalMin?: number;
    };
    expect(stored.usageCollectIntervalMin).toBe(60);
  });

  it('prefers core theme over localStorage and caches it', async () => {
    stubCore({ theme: 'dark' });
    localStorage.setItem(StorageKey.theme, 'light');

    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.theme).toBe('dark');
    expect(localStorage.getItem(StorageKey.theme)).toBe('dark');
  });

  it('migrates local interval to core when the field is omitted', async () => {
    stubCore();
    localStorage.setItem(settingsKey, JSON.stringify({ usageCollectIntervalMin: 15 }));

    const port = createTauriSettingsPort();
    const s = await port.getSettings();
    expect(s.usageCollectIntervalMin).toBe(15);
    expect(invokeMock).toHaveBeenCalledWith('set_setting', {
      key: 'usage_collect_interval_min',
      value: '15',
    });
  });
});

describe('createTauriSettingsPort pickDirectory', () => {
  beforeEach(() => {
    tauriRuntime = true;
    invokeMock.mockReset();
  });

  it('invokes pick_directory and returns a path', async () => {
    invokeMock.mockResolvedValue('/Users/me/proj');
    const port = createTauriSettingsPort();
    await expect(
      port.pickDirectory({ title: '选择工作目录', defaultPath: '/Users/me' }),
    ).resolves.toBe('/Users/me/proj');
    expect(invokeMock).toHaveBeenCalledWith('pick_directory', {
      title: '选择工作目录',
      defaultPath: '/Users/me',
    });
  });

  it('maps cancel and blank to null', async () => {
    const port = createTauriSettingsPort();
    invokeMock.mockResolvedValueOnce(null);
    await expect(port.pickDirectory()).resolves.toBeNull();
    invokeMock.mockResolvedValueOnce('   ');
    await expect(port.pickDirectory()).resolves.toBeNull();
  });
});

describe('isAlreadyDisabledAutostartError', () => {
  it('matches Windows missing-value errors', () => {
    expect(
      isAlreadyDisabledAutostartError(new Error('系统找不到指定的文件。 (os error 2)')),
    ).toBe(true);
    expect(
      isAlreadyDisabledAutostartError(new Error('The system cannot find the file specified. (os error 2)')),
    ).toBe(true);
    expect(isAlreadyDisabledAutostartError(new Error('access denied'))).toBe(false);
  });
});
