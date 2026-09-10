import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('settings layout wiring', () => {
  it('keeps tabs in the workbench header and centers the body; backups keeps the full-height split', () => {
    const page = source('index.tsx');
    expect(page).not.toContain('pageRhythm.readingStart');
    expect(page).not.toContain('pageRhythm.formColumn');
    expect(page.match(/className=\{pageRhythm\.overviewColumn\}/g)?.length).toBe(3);
    expect(page).not.toContain('pageRhythm.readingColumn');
    expect(page).toMatch(/workbenchHeader\}>\s*<div className=\{pageRhythm\.chrome\}/);
    expect(page).not.toMatch(/overviewColumn\}>\s*<PageHeader/);
    expect(page).toContain('TabsContent value="backups" className="h-full min-h-0"');
    expect(page).toContain('toolbar={settingsTabList}');
    expect(page).not.toContain('flushTop');
  });

  it('puts brand-color swatches next to the theme row', () => {
    const prefs = source('PreferencesPanel.tsx');
    expect(prefs).toContain("t('settings.general.accentLabel')");
    expect(prefs).toContain('persistAccent');
    expect(prefs.indexOf("t('settings.general.themeLabel')")).toBeLessThan(
      prefs.indexOf("t('settings.general.accentLabel')"),
    );
    expect(prefs).toContain("t('settings.general.canvasLabel')");
    expect(prefs).toContain('persistCanvas');
    expect(prefs.indexOf("t('settings.general.accentLabel')")).toBeLessThan(
      prefs.indexOf("t('settings.general.canvasLabel')"),
    );
  });

  it('puts sidebar page toggles on the Features tab, not Preferences', () => {
    const page = source('index.tsx');
    const features = source('FeaturesPanel.tsx');
    const prefs = source('PreferencesPanel.tsx');
    const format = source('settings-format.ts');
    expect(page).toContain('value="features"');
    expect(page).toContain("t('settings.page.tabFeatures')");
    expect(page).toContain('<FeaturesPanel />');
    expect(prefs).not.toContain('OPTIONAL_NAV_IDS');
    expect(prefs).not.toContain('setNavVisible');
    expect(prefs).not.toContain("t('settings.general.sectionSidebar')");
    expect(features).toContain("t('settings.general.sectionSidebarNav')");
    expect(features).toContain('OPTIONAL_NAV_IDS');
    expect(features).toContain('OPTIONAL_NAV_VISIBLE_COPY');
    expect(features).toContain('setNavVisible');
    expect(features).toContain("t('settings.general.autoCollapseOnRoutesLabel')");
    expect(features.indexOf("t('settings.general.sectionSidebarNav')")).toBeLessThan(
      features.indexOf("t('settings.general.sectionSidebar')"),
    );
    expect(format).toContain('skillsNavVisibleLabel');
    expect(format).toContain('mcpNavVisibleLabel');
    expect(format).toContain('projectsNavVisibleLabel');
    expect(format).toContain('pluginsNavVisibleLabel');
    expect(format).toContain('connectionsNavVisibleLabel');
    expect(format).toContain('sub2apiNavVisibleLabel');
    expect(format).toContain('routesNavVisibleLabel');
    const skillsAt = format.indexOf('skillsNavVisibleLabel');
    const mcpAt = format.indexOf('mcpNavVisibleLabel');
    const projectsAt = format.indexOf('projectsNavVisibleLabel');
    const pluginsAt = format.indexOf('pluginsNavVisibleLabel');
    const connectionsAt = format.indexOf('connectionsNavVisibleLabel');
    const sub2apiAt = format.indexOf('sub2apiNavVisibleLabel');
    const routesAt = format.indexOf('routesNavVisibleLabel');
    expect(skillsAt).toBeLessThan(mcpAt);
    expect(mcpAt).toBeLessThan(projectsAt);
    expect(projectsAt).toBeLessThan(pluginsAt);
    expect(pluginsAt).toBeLessThan(connectionsAt);
    expect(connectionsAt).toBeLessThan(sub2apiAt);
    expect(sub2apiAt).toBeLessThan(routesAt);
  });

  it('marks plugins toggle as in development', () => {
    const features = source('FeaturesPanel.tsx');
    expect(features).toContain("t('common.inDevelopment')");
    expect(features).toContain("id === 'plugins'");
    expect((features.match(/t\('common\.inDevelopment'\)/g) ?? []).length).toBe(1);
    expect(features).toContain("aria-label={t('settings.general.autoCollapseOnRoutesLabel')}");
    expect(features).toContain('aria-label={label}');
  });

  it('names launch and route switches for assistive reading', () => {
    const prefs = source('PreferencesPanel.tsx');
    const shared = source('settings-shared.tsx');
    expect(prefs).toContain("t('settings.general.autoStartLabel')");
    expect(prefs).toContain("t('settings.general.closeToTrayLabel')");
    expect(prefs).not.toContain("aria-label={t('settings.general.autoStartLabel')}");
    expect(prefs).not.toContain("aria-label={t('settings.general.closeToTrayLabel')}");
    expect(prefs).not.toContain("aria-label={t('settings.data.usageIntervalLabel')}");
    expect(shared).toContain('aria-labelledby');
    expect(shared).toContain('aria-describedby');
  });

  it('groups preference rows into labeled sections in a stable order', () => {
    const prefs = source('PreferencesPanel.tsx');
    const shared = source('settings-shared.tsx');
    expect(shared).toContain('flex gap-4 py-2');
    expect(shared).toContain('export function SettingsGroup');
    expect(prefs).toContain("t('settings.general.sectionAppearance')");
    expect(prefs).toContain('SettingsGroup first');
    const keys = [
      'sectionAppearance',
      'sectionLaunch',
      'sectionRoutes',
      'sectionSkills',
      'sectionUsage',
    ];
    const indexes = keys.map((key) => prefs.indexOf(`t('settings.general.${key}')`));
    for (const index of indexes) expect(index).toBeGreaterThan(-1);
    for (let i = 1; i < indexes.length; i += 1) {
      expect(indexes[i - 1]).toBeLessThan(indexes[i]);
    }
  });
});
