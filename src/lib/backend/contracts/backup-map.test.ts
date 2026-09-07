import { describe, expect, it } from 'vitest';
import {
  mapCoreRestoreResult,
  restoreHasDeleteFailures,
  type CoreRestoreResult,
} from './backup-map';

function restoreResult(
  skippedDeletions?: CoreRestoreResult['skippedDeletions'],
): CoreRestoreResult {
  return {
    restored: {
      id: 'bk-1',
      agentId: 'claude',
      kind: 'manual',
      path: '/tmp/bk-1',
      files: ['settings.json'],
      size: 4,
      createdAt: '2026-09-07T00:00:00Z',
    },
    restoredPaths: ['/tmp/live/settings.json'],
    skippedDeletions,
  };
}

describe('CoreRestoreResult skippedDeletions', () => {
  it('mapCoreRestoreResult keeps skippedDeletions and defaults a missing list', () => {
    const withSkips = mapCoreRestoreResult(
      restoreResult([{ path: '/tmp/live/auth.json', reason: 'delete_failed' }]),
    );
    expect(withSkips.skippedDeletions).toEqual([
      { path: '/tmp/live/auth.json', reason: 'delete_failed' },
    ]);

    const omitted = mapCoreRestoreResult({
      restored: restoreResult().restored,
      restoredPaths: ['/tmp/live/settings.json'],
    });
    expect(omitted.skippedDeletions).toEqual([]);
  });

  it('restoreHasDeleteFailures is true only for delete_failed', () => {
    expect(
      restoreHasDeleteFailures(
        restoreResult([{ path: '/tmp/live/auth.json', reason: 'delete_failed' }]),
      ),
    ).toBe(true);
    expect(
      restoreHasDeleteFailures(
        restoreResult([
          { path: '/tmp/live/auth.json', reason: 'edited' },
          { path: '/tmp/live/extra.json', reason: 'unknown' },
        ]),
      ),
    ).toBe(false);
    expect(restoreHasDeleteFailures(restoreResult([]))).toBe(false);
    expect(restoreHasDeleteFailures(restoreResult(undefined))).toBe(false);
  });
});
