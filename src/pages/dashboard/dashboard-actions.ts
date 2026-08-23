import { settingsSearch } from '@/pages/settings/settings-format';

/** Dashboard「立即备份」opens Settings backups; it must not snapshot agents here. */
export function dashboardBackupNowHref(): string {
  return `/settings${settingsSearch('backups')}`;
}
