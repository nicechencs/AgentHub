import { describe, expect, it } from 'vitest';
import { flattenKeys } from '@/lib/i18n';
import { zh } from '@/lib/i18n/locales/zh';
import {
  NAV_MANAGE,
  NAV_WORKSPACE,
} from '@/components/layout/sidebar-nav';
import {
  ROUTES_ACTIVITY_PATH,
  ROUTES_BOARD_PATH,
  ROUTES_PATH,
  ROUTES_POOL_PATH,
  ROUTES_TOKENS_PATH,
  SUB2API_PATH,
} from '@/lib/routes-path';
import { ROUTES_NAV_ITEMS } from '@/pages/routes/routes-nav-items';
import { SETTINGS_TABS } from '@/pages/settings/settings-format';
import { SKILL_TABS } from '@/pages/skills/skills-preview-model';
import {
  PAGE_HELP_IDS,
  PAGE_HELP_STEP_COUNT,
  PAGE_HELP_TARGETS,
  pageHelpCopy,
  pageHelpIdFromPath,
  pageHelpStepSelector,
} from './page-help-model';

describe('pageHelpIdFromPath', () => {
  it('maps each primary page to its tutorial', () => {
    expect(pageHelpIdFromPath('/')).toBe('dashboard');
    expect(pageHelpIdFromPath('/chat')).toBe('chat');
    expect(pageHelpIdFromPath('/agents')).toBe('agents');
    expect(pageHelpIdFromPath('/skills')).toBe('skillsLibrary');
    expect(pageHelpIdFromPath('/mcp')).toBe('mcp');
    expect(pageHelpIdFromPath('/projects')).toBe('projects');
    expect(pageHelpIdFromPath('/plugins')).toBe('plugins');
    expect(pageHelpIdFromPath('/connections')).toBe('connections');
    expect(pageHelpIdFromPath(SUB2API_PATH)).toBe('sub2api');
    expect(pageHelpIdFromPath('/settings')).toBe('settingsPreferences');
  });

  it('maps Skills tabs from ?tab=', () => {
    expect(pageHelpIdFromPath('/skills', '?tab=library')).toBe('skillsLibrary');
    expect(pageHelpIdFromPath('/skills', '?tab=project')).toBe('skillsProject');
    expect(pageHelpIdFromPath('/skills', '?tab=market')).toBe('skillsMarket');
    expect(pageHelpIdFromPath('/skills', '?tab=workspace')).toBe('skillsLibrary');
  });

  it('maps Settings tabs from ?tab=', () => {
    expect(pageHelpIdFromPath('/settings', '?tab=preferences')).toBe('settingsPreferences');
    expect(pageHelpIdFromPath('/settings', '?tab=local')).toBe('settingsLocal');
    expect(pageHelpIdFromPath('/settings', '?tab=backups')).toBe('settingsBackups');
    expect(pageHelpIdFromPath('/settings', '?tab=about')).toBe('settingsAbout');
    expect(pageHelpIdFromPath('/settings', '?tab=general')).toBe('settingsPreferences');
    expect(pageHelpIdFromPath('/settings', '?tab=data')).toBe('settingsLocal');
    expect(pageHelpIdFromPath('/settings', '?tab=security')).toBe('settingsAbout');
  });

  it('maps Routes subpages, including the area root, to their own tutorials', () => {
    expect(pageHelpIdFromPath(ROUTES_PATH)).toBe('routesBoard');
    expect(pageHelpIdFromPath(ROUTES_BOARD_PATH)).toBe('routesBoard');
    expect(pageHelpIdFromPath(ROUTES_POOL_PATH)).toBe('routesPool');
    expect(pageHelpIdFromPath(`${ROUTES_POOL_PATH}/x`)).toBe('routesPool');
    expect(pageHelpIdFromPath(ROUTES_TOKENS_PATH)).toBe('routesTokens');
    expect(pageHelpIdFromPath(ROUTES_ACTIVITY_PATH)).toBe('routesActivity');
  });

  it('falls back to Dashboard for unknown paths', () => {
    expect(pageHelpIdFromPath('/not-a-page')).toBe('dashboard');
  });

  it('covers every sidebar, Routes, Settings, and Skills surface', () => {
    const mapped = new Set<string>();
    for (const item of [...NAV_WORKSPACE, ...NAV_MANAGE]) {
      mapped.add(pageHelpIdFromPath(item.to));
    }
    for (const item of ROUTES_NAV_ITEMS) {
      mapped.add(pageHelpIdFromPath(item.to));
    }
    for (const tab of SETTINGS_TABS) {
      mapped.add(pageHelpIdFromPath('/settings', `?tab=${tab}`));
    }
    for (const tab of SKILL_TABS) {
      mapped.add(pageHelpIdFromPath('/skills', `?tab=${tab}`));
    }
    expect(mapped).toEqual(new Set(PAGE_HELP_IDS));
  });

  it('exposes title, intro, and matching steps for every page', () => {
    const keys = flattenKeys(zh);
    expect(PAGE_HELP_IDS).toHaveLength(19);
    for (const id of PAGE_HELP_IDS) {
      const copy = pageHelpCopy(id);
      const n = PAGE_HELP_STEP_COUNT[id];
      expect(copy.title).toBe(`chrome.pageHelp.pages.${id}.title`);
      expect(copy.intro).toBe(`chrome.pageHelp.pages.${id}.intro`);
      expect(copy.steps).toHaveLength(n);
      expect(keys).toContain(copy.title);
      expect(keys).toContain(copy.intro);
      for (const step of copy.steps) {
        expect(keys).toContain(step);
      }
      expect(PAGE_HELP_TARGETS[id]).toHaveLength(n);
      expect(pageHelpStepSelector(id, 0)).toMatch(/^\[data-help="/);
      expect(new Set(PAGE_HELP_TARGETS[id]).size, id).toBe(PAGE_HELP_TARGETS[id].length);
    }
  });
});
