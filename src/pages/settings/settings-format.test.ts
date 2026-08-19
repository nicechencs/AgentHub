import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  SETTINGS_TABS,
  clampLogRetentionDays,
  clampUsageIntervalMin,
  logLevelOptionLabel,
  parseSettingsTab,
  resolveSettingsLocation,
  settingsSearch,
  skillMarketLabel,
} from './settings-format';

const tZh = createTranslator('zh');
const tEn = createTranslator('en');

describe('parseSettingsTab', () => {
  it('accepts canonical slugs', () => {
    expect(SETTINGS_TABS).toEqual(['preferences', 'local', 'about']);
    expect(parseSettingsTab('preferences')).toBe('preferences');
    expect(parseSettingsTab('local')).toBe('local');
    expect(parseSettingsTab('about')).toBe('about');
  });

  it('maps legacy slugs onto the three-tab IA', () => {
    expect(parseSettingsTab('general')).toBe('preferences');
    expect(parseSettingsTab('security')).toBe('about');
    expect(parseSettingsTab('data')).toBe('local');
    expect(parseSettingsTab('backups')).toBe('local');
  });

  it('falls back to preferences for empty or unknown values', () => {
    expect(parseSettingsTab(null)).toBe('preferences');
    expect(parseSettingsTab('')).toBe('preferences');
    expect(parseSettingsTab('nope')).toBe('preferences');
  });
});

describe('resolveSettingsLocation', () => {
  it('does not rewrite a bare /settings URL', () => {
    expect(resolveSettingsLocation(null)).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: false,
    });
  });

  it('leaves canonical slugs in place', () => {
    expect(resolveSettingsLocation('preferences')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('local')).toEqual({
      tab: 'local',
      hash: '',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('about')).toEqual({
      tab: 'about',
      hash: '',
      shouldReplace: false,
    });
  });

  it('replace-navigates legacy slugs', () => {
    expect(resolveSettingsLocation('general')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('security')).toEqual({
      tab: 'about',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('data')).toEqual({
      tab: 'local',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('backups')).toEqual({
      tab: 'local',
      hash: 'backups',
      shouldReplace: true,
    });
  });

  it('replace-navigates unknown slugs to preferences', () => {
    expect(resolveSettingsLocation('legacy-foo')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: true,
    });
  });

  it('builds search strings', () => {
    expect(settingsSearch('local')).toBe('?tab=local');
  });
});

describe('legacy backups hash', () => {
  it('maps backups slug and #backups onto local#backups', () => {
    expect(resolveSettingsLocation('local', '#backups')).toEqual({
      tab: 'local',
      hash: 'backups',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation(null, 'backups')).toEqual({
      tab: 'local',
      hash: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('backups', '#backups')).toEqual({
      tab: 'local',
      hash: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('preferences', '#backups')).toEqual({
      tab: 'local',
      hash: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('local', '')).toEqual({
      tab: 'local',
      hash: '',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('preferences', '#other')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: false,
    });
  });
});

describe('settings-format i18n helpers', () => {
  it('does not list backups as a peer Settings tab', () => {
    expect(tZh('settings.page.description')).toBe('偏好、本机与关于');
    expect(tEn('settings.page.description')).toBe('Preferences, this device, and about');
    expect(tZh('settings.page.description')).not.toContain('备份');
    expect(tEn('settings.page.description')).not.toMatch(/backup/i);
    expect(tZh('settings.page.descriptionTip')).toContain('本机含数据目录、日志与备份');
    expect(tEn('settings.page.descriptionTip')).toContain('This device covers data, logs, and backups');
  });

  it('labels follow the active language', () => {
    expect(skillMarketLabel('auto', tZh)).toBe('自动（不通则切换）');
    expect(skillMarketLabel('auto', tEn)).toBe('Auto (switch if unreachable)');
    expect(logLevelOptionLabel('info', tEn)).toBe('info — normal (default)');
  });

  it('clamps retention days and usage interval', () => {
    expect(clampLogRetentionDays(0)).toBe(1);
    expect(clampLogRetentionDays(14)).toBe(14);
    expect(clampLogRetentionDays(400)).toBe(365);
    expect(clampUsageIntervalMin(-1)).toBe(0);
    expect(clampUsageIntervalMin(30)).toBe(30);
    expect(clampUsageIntervalMin(2000)).toBe(24 * 60);
  });
});
