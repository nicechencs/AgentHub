import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppSettings } from '@/lib/types';

const updateSettings = vi.fn();

vi.mock('@/lib/api/settings', () => ({
  updateSettings: (...args: unknown[]) => updateSettings(...args),
}));

const sample: AppSettings = {
  language: 'zh',
  theme: 'light',
  autoStart: true,
  closeToTray: true,
  dataDir: '~/.agenthub',
  logsDir: '~/.agenthub/logs',
  logLevel: 'info',
  logRetentionDays: 14,
  skillMarketSource: 'auto',
  usageCollectIntervalMin: 30,
  appVersion: '0.0.0',
};

describe('persistSettingsPatch', () => {
  beforeEach(() => {
    updateSettings.mockReset();
    updateSettings.mockImplementation(async (patch: Partial<AppSettings>) => ({
      ...sample,
      ...patch,
    }));
  });

  it('writes the patch immediately and returns the saved settings', async () => {
    const { persistSettingsPatch } = await import('./settings-persist');
    const saved = await persistSettingsPatch({ theme: 'dark' });
    expect(updateSettings).toHaveBeenCalledWith({ theme: 'dark' });
    expect(saved.theme).toBe('dark');
  });
});
