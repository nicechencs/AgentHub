import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { AppSettings } from '@/lib/types';
import {
  dataSettingsSaveDescription,
  generalSettingsPayload,
  generalSettingsSaveDescription,
  logLevelOptionLabel,
  skillMarketLabel,
} from './settings-format';

const tZh = createTranslator('zh');
const tEn = createTranslator('en');

const sample: AppSettings = {
  language: 'en',
  theme: 'dark',
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

describe('settings-format i18n helpers', () => {
  it('general save payload includes language', () => {
    expect(generalSettingsPayload(sample)).toEqual({
      theme: 'dark',
      language: 'en',
      autoStart: true,
      closeToTray: true,
      skillMarketSource: 'auto',
    });
  });

  it('labels and save summaries follow the active language', () => {
    expect(skillMarketLabel('auto', tZh)).toBe('自动（不通则切换）');
    expect(skillMarketLabel('auto', tEn)).toBe('Auto (switch if unreachable)');
    expect(logLevelOptionLabel('info', tEn)).toBe('info — normal (default)');
    expect(generalSettingsSaveDescription(sample, tEn)).toContain('Close to tray');
    expect(dataSettingsSaveDescription(sample, tEn)).toContain('Auto-collect usage every 30 min');
  });
});
