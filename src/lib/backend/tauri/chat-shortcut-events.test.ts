import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';

const { isTauriMock, listenMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

import {
  CHAT_SHORTCUT_EVENT,
  chatNativeShortcutFromPayload,
  onChatNativeShortcut,
} from './chat-shortcut-events';

describe('chatNativeShortcutFromPayload', () => {
  it('accepts newChat only', () => {
    expect(chatNativeShortcutFromPayload({ action: 'newChat' })).toBe('newChat');
    expect(chatNativeShortcutFromPayload({ action: 'overview' })).toBeNull();
    expect(chatNativeShortcutFromPayload({})).toBeNull();
    expect(chatNativeShortcutFromPayload(undefined)).toBeNull();
  });
});

describe('onChatNativeShortcut', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onChatNativeShortcut(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
  });

  it('forwards the menu event', async () => {
    isTauriMock.mockReturnValue(true);
    let captured: ((event: { payload: { action: string } }) => void) | undefined;
    listenMock.mockImplementation(async (_name: string, cb: (event: { payload: { action: string } }) => void) => {
      captured = cb;
      return () => {};
    });
    const handler = vi.fn();
    await onChatNativeShortcut(handler);
    expect(listenMock).toHaveBeenCalledWith(CHAT_SHORTCUT_EVENT, expect.any(Function));
    captured?.({ payload: { action: 'newChat' } });
    expect(handler).toHaveBeenCalledWith('newChat');
  });
});
