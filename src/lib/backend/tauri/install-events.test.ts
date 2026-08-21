import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';

const { isTauriMock, listenMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

import { onInstallProgress } from './install-events';

describe('tauri install events', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
  });

  it('fails closed outside Tauri instead of returning a fake unsubscribe', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onInstallProgress(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
  });

  it('surfaces listen initialization failure as backend unavailable', async () => {
    isTauriMock.mockReturnValue(true);
    listenMock.mockRejectedValue(new Error('listen failed'));
    await expect(onInstallProgress(() => {})).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
  });
});
