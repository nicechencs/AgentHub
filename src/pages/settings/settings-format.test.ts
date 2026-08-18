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
    expect(SETTINGS_TABS).toEqual(['preferences', 'local', 'backups', 'about']);
    expect(parseSettingsTab('preferences')).toBe('preferences');
    expect(parseSettingsTab('local')).toBe('local');
    expect(parseSettingsTab('backups')).toBe('backups');
    expect(parseSettingsTab('about')).toBe('about');
  });

  it('maps legacy slugs onto the four-tab IA', () => {
    expect(parseSettingsTab('general')).toBe('preferences');
    expect(parseSettingsTab('security')).toBe('about');
    expect(parseSettingsTab('data')).toBe('local');
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
      shouldReplace: false,
    });
  });

  it('leaves canonical slugs in place', () => {
    expect(resolveSettingsLocation('preferences')).toEqual({
      tab: 'preferences',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('local')).toEqual({
      tab: 'local',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('about')).toEqual({
      tab: 'about',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('backups')).toEqual({
      tab: 'backups',
      shouldReplace: false,
    });
  });

  it('replace-navigates legacy slugs', () => {
    expect(resolveSettingsLocation('general')).toEqual({
      tab: 'preferences',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('security')).toEqual({
      tab: 'about',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('data')).toEqual({
      tab: 'local',
      shouldReplace: true,
    });
  });

  it('replace-navigates unknown slugs to preferences', () => {
    expect(resolveSettingsLocation('legacy-foo')).toEqual({
      tab: 'preferences',
      shouldReplace: true,
    });
  });

  it('builds search strings', () => {
    expect(settingsSearch('local')).toBe('?tab=local');
  });
});

describe('legacy backups hash', () => {
  it('maps #backups onto the backups tab and replace-navigates', () => {
    expect(resolveSettingsLocation('local', '#backups')).toEqual({
      tab: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation(null, 'backups')).toEqual({
      tab: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('backups', '#backups')).toEqual({
      tab: 'backups',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('local', '')).toEqual({
      tab: 'local',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('preferences', '#other')).toEqual({
      tab: 'preferences',
      shouldReplace: false,
    });
  });
});

describe('settings-format i18n helpers', () => {
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
