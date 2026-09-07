import { describe, expect, it, vi } from 'vitest';
import type { BackupPort } from '@/lib/backend/contracts';
import { createMockBackupPort } from './backup';

vi.mock('@/dev/mocks/delay', () => ({
  delay: async () => {},
  randomLatency: () => 0,
}));

describe('mock backup restore contract', () => {
  it('restoreBackup returns skippedDeletions on the BackupPort restore result', async () => {
    const mock: BackupPort = createMockBackupPort();
    const result = await mock.restoreBackup('bk-1');
    expect(result).toEqual(
      expect.objectContaining({
        restored: expect.objectContaining({
          id: 'bk-1',
          agentId: 'claude',
          files: expect.any(Array),
        }),
        restoredPaths: expect.any(Array),
        skippedDeletions: [],
      }),
    );
    expect(Array.isArray(result.skippedDeletions)).toBe(true);
  });
});
