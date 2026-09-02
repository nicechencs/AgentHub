import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';

const { isTauriMock, listenMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

import {
  LOCAL_FORWARD_LIFECYCLE_EVENT,
  localForwardLifecyclePhase,
  onLocalForwardLifecycle,
} from './local-forward-events';

describe('localForwardLifecyclePhase', () => {
  it('accepts restarting and ready', () => {
    expect(localForwardLifecyclePhase({ phase: 'restarting' })).toBe('restarting');
    expect(localForwardLifecyclePhase({ phase: 'ready' })).toBe('ready');
  });

  it('rejects missing, empty, and unknown phases', () => {
    expect(localForwardLifecyclePhase(undefined)).toBeNull();
    expect(localForwardLifecyclePhase({})).toBeNull();
    expect(localForwardLifecyclePhase({ phase: '' })).toBeNull();
    expect(localForwardLifecyclePhase({ phase: 'stopped' })).toBeNull();
    expect(localForwardLifecyclePhase({ phase: 1 })).toBeNull();
  });
});

describe('tauri local-forward lifecycle events', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onLocalForwardLifecycle(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
  });

  it('surfaces listener initialization errors', async () => {
    isTauriMock.mockReturnValue(true);
    listenMock.mockRejectedValue(new Error('listen failed'));
    await expect(onLocalForwardLifecycle(() => {})).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
  });

  it('forwards restarting and ready payloads and ignores unknown phases', async () => {
    isTauriMock.mockReturnValue(true);
    let listener: ((event: { payload: { phase?: unknown } }) => void) | undefined;
    listenMock.mockImplementation(async (event: string, handler: typeof listener) => {
      expect(event).toBe(LOCAL_FORWARD_LIFECYCLE_EVENT);
      listener = handler;
      return () => {};
    });

    const seen: string[] = [];
    await onLocalForwardLifecycle((payload) => {
      seen.push(payload.phase);
    });

    listener?.({ payload: { phase: 'restarting' } });
    listener?.({ payload: { phase: 'ready' } });
    listener?.({ payload: { phase: 'stopped' } });
    listener?.({ payload: {} });

    expect(seen).toEqual(['restarting', 'ready']);
  });
});
