import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StorageKey } from '@/lib/storage-key';
import {
  clampProcessLogHeight,
  persistProcessLogHeight,
  PROCESS_LOG_SPECS,
  processLogHeightOrDefault,
  readStoredProcessLogHeight,
} from './chat-process-log-model';

describe('clampProcessLogHeight', () => {
  it('keeps a mid-range command height', () => {
    expect(clampProcessLogHeight('command', 200)).toBe(200);
  });

  it('does not shrink below the command floor', () => {
    expect(clampProcessLogHeight('command', 10)).toBe(PROCESS_LOG_SPECS.command.minHeight);
  });

  it('does not grow past the process log ceiling', () => {
    expect(clampProcessLogHeight('stderr', 4000)).toBe(PROCESS_LOG_SPECS.stderr.maxHeight);
  });
});

describe('process log height persistence', () => {
  const store = new Map<string, string>();
  const localStorage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  };

  beforeEach(() => {
    store.clear();
    vi.stubGlobal('window', { localStorage });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns null when nothing is stored', () => {
    expect(readStoredProcessLogHeight('command')).toBeNull();
    expect(readStoredProcessLogHeight('stderr')).toBeNull();
  });

  it('round-trips command and process log heights separately', () => {
    persistProcessLogHeight('command', 220);
    persistProcessLogHeight('stderr', 360);
    expect(readStoredProcessLogHeight('command')).toBe(220);
    expect(readStoredProcessLogHeight('stderr')).toBe(360);
    expect(store.get(StorageKey.chatProcessCommandHeight)).toBe('220');
    expect(store.get(StorageKey.chatProcessLogHeight)).toBe('360');
  });

  it('clears a remembered height', () => {
    persistProcessLogHeight('stderr', 360);
    persistProcessLogHeight('stderr', null);
    expect(readStoredProcessLogHeight('stderr')).toBeNull();
    expect(processLogHeightOrDefault('stderr', null)).toBe(PROCESS_LOG_SPECS.stderr.defaultHeight);
  });
});
