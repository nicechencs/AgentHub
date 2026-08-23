import { describe, expect, it } from 'vitest';
import { dashboardBackupNowHref } from './dashboard-actions';

describe('dashboardBackupNowHref', () => {
  it('sends 立即备份 to Settings backups instead of backing up the first agent', () => {
    expect(dashboardBackupNowHref()).toBe('/settings?tab=backups');
  });
});
