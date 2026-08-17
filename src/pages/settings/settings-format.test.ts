import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  SETTINGS_BACKUPS_HASH,
  SETTINGS_TABS,
  clampLogRetentionDays,
  clampUsageIntervalMin,
  logLevelOptionLabel,
  parseSettingsHash,
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
    expect(resolveSettingsLocation(null, '')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: false,
    });
  });

  it('leaves canonical slugs in place', () => {
    expect(resolveSettingsLocation('preferences', '')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('local', '')).toEqual({
      tab: 'local',
      hash: '',
      shouldReplace: false,
    });
    expect(resolveSettingsLocation('about', '')).toEqual({
      tab: 'about',
      hash: '',
      shouldReplace: false,
    });
  });

  it('replace-navigates legacy slugs', () => {
    expect(resolveSettingsLocation('general', '')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('security', '')).toEqual({
      tab: 'about',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('data', '')).toEqual({
      tab: 'local',
      hash: '',
      shouldReplace: true,
    });
  });

  it('sends tab=backups to local and focuses the backups section', () => {
    expect(resolveSettingsLocation('backups', '')).toEqual({
      tab: 'local',
      hash: SETTINGS_BACKUPS_HASH,
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('local', '#backups')).toEqual({
      tab: 'local',
      hash: SETTINGS_BACKUPS_HASH,
      shouldReplace: false,
    });
  });

  it('replace-navigates unknown slugs to preferences', () => {
    expect(resolveSettingsLocation('legacy-foo', '')).toEqual({
      tab: 'preferences',
      hash: '',
      shouldReplace: true,
    });
  });

  it('builds search strings and parses the backups hash', () => {
    expect(settingsSearch('local')).toBe('?tab=local');
    expect(parseSettingsHash('#backups')).toBe('backups');
    expect(parseSettingsHash('backups')).toBe('backups');
    expect(parseSettingsHash('#other')).toBeNull();
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
