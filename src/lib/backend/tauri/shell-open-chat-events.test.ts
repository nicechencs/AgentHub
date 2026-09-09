import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';

const { isTauriMock, listenMock, invokeMock, onFocusChangedMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
  invokeMock: vi.fn(),
  onFocusChangedMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onFocusChanged: onFocusChangedMock }),
}));
vi.mock('./invoke', () => ({ invoke: invokeMock }));

import {
  onOpenChatCwd,
  openChatCwdFromPayload,
  subscribeOpenChatCwdWakeups,
  takePendingOpenChatCwd,
} from './shell-open-chat-events';

describe('openChatCwdFromPayload', () => {
  it('accepts a non-empty folder path', () => {
    expect(openChatCwdFromPayload({ cwd: 'D:\\work\\app' })).toBe('D:\\work\\app');
    expect(openChatCwdFromPayload({ cwd: '  /tmp/app  ' })).toBe('/tmp/app');
  });

  it('rejects missing, empty, and non-string paths', () => {
    expect(openChatCwdFromPayload(undefined)).toBeNull();
    expect(openChatCwdFromPayload({})).toBeNull();
    expect(openChatCwdFromPayload({ cwd: '' })).toBeNull();
    expect(openChatCwdFromPayload({ cwd: '   ' })).toBeNull();
    expect(openChatCwdFromPayload({ cwd: 1 })).toBeNull();
  });
});

describe('tauri open-chat-cwd events', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
    invokeMock.mockReset();
    onFocusChangedMock.mockReset();
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onOpenChatCwd(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
    await expect(takePendingOpenChatCwd()).rejects.toBeInstanceOf(BackendUnavailableError);
    await expect(subscribeOpenChatCwdWakeups(() => {})).rejects.toBeInstanceOf(
      BackendUnavailableError,
    );
  });

  it('returns a pending folder from the desktop command', async () => {
    isTauriMock.mockReturnValue(true);
    invokeMock.mockResolvedValue('D:\\work\\app');
    await expect(takePendingOpenChatCwd()).resolves.toBe('D:\\work\\app');
  });

  it('wakes on the folder event even when the payload has no cwd', async () => {
    isTauriMock.mockReturnValue(true);
    let captured: (() => void) | undefined;
    listenMock.mockImplementation(async (_name: string, cb: () => void) => {
      captured = cb;
      return () => {};
    });
    const handler = vi.fn();
    await onOpenChatCwd(handler);
    captured?.();
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('wakes again when the window is focused or the page becomes visible', async () => {
    isTauriMock.mockReturnValue(true);
    listenMock.mockResolvedValue(() => {});
    let onFocus: ((event: { payload: boolean }) => void) | undefined;
    onFocusChangedMock.mockImplementation(async (cb: (event: { payload: boolean }) => void) => {
      onFocus = cb;
      return () => {};
    });

    const visibilityListeners = new Map<string, () => void>();
    const doc = {
      visibilityState: 'hidden' as Document['visibilityState'],
      addEventListener: (type: string, cb: () => void) => {
        visibilityListeners.set(type, cb);
      },
      removeEventListener: (type: string) => {
        visibilityListeners.delete(type);
      },
    };
    vi.stubGlobal('document', doc);

    const handler = vi.fn();
    const unsub = await subscribeOpenChatCwdWakeups(handler);

    onFocus?.({ payload: false });
    expect(handler).not.toHaveBeenCalled();
    onFocus?.({ payload: true });
    expect(handler).toHaveBeenCalledTimes(1);

    visibilityListeners.get('visibilitychange')?.();
    expect(handler).toHaveBeenCalledTimes(1);
    doc.visibilityState = 'visible';
    visibilityListeners.get('visibilitychange')?.();
    expect(handler).toHaveBeenCalledTimes(2);

    unsub();
    vi.unstubAllGlobals();
  });
});
