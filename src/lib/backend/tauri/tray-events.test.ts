import { beforeEach, describe, expect, it, vi } from 'vitest';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';
import { BRIDGES_PATH } from '@/lib/bridges-path';

const { isTauriMock, listenMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

import { onTrayNavigate, trayNavigatePath } from './tray-events';

describe('trayNavigatePath', () => {
  it('accepts /routes', () => {
    expect(trayNavigatePath({ path: '/routes' })).toBe('/routes');
  });

  it('accepts BRIDGES_PATH', () => {
    expect(trayNavigatePath({ path: BRIDGES_PATH })).toBe('/routes');
  });

  it('rejects missing, empty, relative, and non-string paths', () => {
    expect(trayNavigatePath(undefined)).toBeNull();
    expect(trayNavigatePath({})).toBeNull();
    expect(trayNavigatePath({ path: '' })).toBeNull();
    expect(trayNavigatePath({ path: 'routes' })).toBeNull();
    expect(trayNavigatePath({ path: './routes' })).toBeNull();
    expect(trayNavigatePath({ path: 1 })).toBeNull();
    expect(trayNavigatePath({ path: null })).toBeNull();
  });
});

describe('tauri tray events', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    await expect(onTrayNavigate(() => {})).rejects.toBeInstanceOf(BackendUnavailableError);
  });

  it('surfaces listener initialization errors', async () => {
    isTauriMock.mockReturnValue(true);
    listenMock.mockRejectedValue(new Error('listen failed'));
    await expect(onTrayNavigate(() => {})).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
  });
});
