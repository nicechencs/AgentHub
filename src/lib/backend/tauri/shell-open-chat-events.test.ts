import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';

const { isTauriMock, listenMock, invokeMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
  invokeMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));
vi.mock('./invoke', () => ({ invoke: invokeMock }));

import {
  onOpenChatCwd,
  openChatCwdFromPayload,
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
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onOpenChatCwd(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
    await expect(takePendingOpenChatCwd()).rejects.toBeInstanceOf(BackendUnavailableError);
  });

  it('returns a pending folder from the desktop command', async () => {
    isTauriMock.mockReturnValue(true);
    invokeMock.mockResolvedValue('D:\\work\\app');
    await expect(takePendingOpenChatCwd()).resolves.toBe('D:\\work\\app');
  });
});
