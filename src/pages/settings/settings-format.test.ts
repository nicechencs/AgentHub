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
    expect(resolveSettingsLocation('backups')).toEqual({
      tab: 'backups',
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
    expect(settingsSearch('backups')).toBe('?tab=backups');
  });
});

describe('legacy backups hash', () => {
  it('maps #backups and local#backups onto the backups tab', () => {
    expect(resolveSettingsLocation('local', '#backups')).toEqual({
      tab: 'backups',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation(null, 'backups')).toEqual({
      tab: 'backups',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('backups', '#backups')).toEqual({
      tab: 'backups',
      hash: '',
      shouldReplace: true,
    });
    expect(resolveSettingsLocation('preferences', '#backups')).toEqual({
      tab: 'backups',
      hash: '',
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
  it('lists backups as a peer Settings tab', () => {
    expect(tZh('settings.page.description')).toBe('偏好、本机、备份与关于');
    expect(tEn('settings.page.description')).toBe('Preferences, this computer, backups, and about');
    expect(tZh('settings.page.tabLocal')).toBe('本机');
    expect(tEn('settings.page.tabLocal')).toBe('This computer');
    expect(tZh('settings.page.tabBackups')).toBe('备份');
    expect(tEn('settings.page.tabBackups')).toBe('Backups');
    expect(tZh('settings.page.descriptionTip')).toContain('备份管理配置快照');
    expect(tEn('settings.page.descriptionTip')).toContain('Backups manage config snapshots');
  });

  it('labels preference groups in the active language', () => {
    expect(tZh('settings.general.sectionAppearance')).toBe('语言与外观');
    expect(tEn('settings.general.sectionAppearance')).toBe('Language and appearance');
    expect(tZh('settings.general.sectionLaunch')).toBe('启动与关闭');
    expect(tEn('settings.general.sectionLaunch')).toBe('Launch and close');
    expect(tZh('settings.general.sectionSidebar')).toBe('侧栏');
    expect(tEn('settings.general.sectionSidebar')).toBe('Sidebar');
    expect(tZh('settings.general.sectionRoutes')).toBe('路由');
    expect(tEn('settings.general.sectionRoutes')).toBe('Routes');
    expect(tZh('settings.general.sectionSkills')).toBe('技能');
    expect(tEn('settings.general.sectionSkills')).toBe('Skills');
    expect(tZh('settings.general.sectionUsage')).toBe('用量');
    expect(tEn('settings.general.sectionUsage')).toBe('Usage');
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
