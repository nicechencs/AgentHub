import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createTauriBackupPort } from './backup';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

const restored = {
  id: 'bk-1',
  agentId: 'claude' as const,
  kind: 'manual' as const,
  path: '/tmp/bk-1',
  files: ['settings.json'],
  size: 4,
  createdAt: '2026-09-07T00:00:00Z',
};

describe('createTauriBackupPort.restoreBackup', () => {
  it('returns skippedDeletions from restore_backup', async () => {
    invokeMock.mockResolvedValueOnce({
      restored,
      preRestore: null,
      restoredPaths: ['/tmp/live/settings.json'],
      skippedDeletions: [{ path: '/tmp/live/auth.json', reason: 'delete_failed' }],
    });
    const port = createTauriBackupPort();
    await expect(port.restoreBackup('bk-1')).resolves.toMatchObject({
      restored,
      restoredPaths: ['/tmp/live/settings.json'],
      skippedDeletions: [{ path: '/tmp/live/auth.json', reason: 'delete_failed' }],
    });
    expect(invokeMock).toHaveBeenCalledWith('restore_backup', { backupId: 'bk-1' });
  });

  it('defaults skippedDeletions to an empty list when the core omits the field', async () => {
    invokeMock.mockResolvedValueOnce({
      restored,
      restoredPaths: ['/tmp/live/settings.json'],
    });
    const result = await createTauriBackupPort().restoreBackup('bk-1');
    expect(result.skippedDeletions).toEqual([]);
  });
});
