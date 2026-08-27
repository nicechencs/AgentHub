import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createTauriProviderPort,
  CURSOR_LIVE_WRITE_UNSUPPORTED,
  mapProviderSwitchError,
} from './provider';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('mapProviderSwitchError', () => {
  it('maps Cursor unsupported rollback strings to Chinese', () => {
    const mapped = mapProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
    );
    expect(mapped).toBeInstanceOf(Error);
    expect(mapped.message).toBe(CURSOR_LIVE_WRITE_UNSUPPORTED);
  });

  it('maps a Tauri object payload that is not an Error', () => {
    expect(mapProviderSwitchError('cursor', { message: 'unsupported' }).message)
      .toBe(CURSOR_LIVE_WRITE_UNSUPPORTED);
  });

  it('leaves other agents unchanged', () => {
    const err = new Error('disk full [io]');
    expect(mapProviderSwitchError('claude', err)).toBe(err);
  });
});

describe('createTauriProviderPort.switchProvider', () => {
  it('throws a Chinese Error instead of swallowing Cursor unsupported', async () => {
    invokeMock.mockRejectedValueOnce(
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
    );
    const port = createTauriProviderPort();
    await expect(port.switchProvider('cursor', 'p-1')).rejects.toThrow(CURSOR_LIVE_WRITE_UNSUPPORTED);
  });

  it('rethrows non-unsupported failures as Error', async () => {
    invokeMock.mockRejectedValueOnce('provider not found: missing [not_found]');
    const port = createTauriProviderPort();
    await expect(port.switchProvider('claude', 'p-1')).rejects.toThrow('provider not found: missing [not_found]');
  });
});
