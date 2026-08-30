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
    expect(page.match(/className=\{pageRhythm\.readingColumn\}/g)?.length).toBe(3);
    expect(page).toMatch(/workbenchHeader\}>\s*<div className=\{pageRhythm\.chrome\}/);
    expect(page).not.toMatch(/readingColumn\}>\s*<PageHeader/);
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
  });

  it('exposes a plugins nav toggle next to the routes toggle', () => {
    const prefs = source('PreferencesPanel.tsx');
    expect(prefs).toContain("t('settings.general.routesNavVisibleLabel')");
    expect(prefs).toContain("t('settings.general.pluginsNavVisibleLabel')");
    expect(prefs).toContain('setPluginsNavVisible');
    expect(prefs.indexOf("t('settings.general.routesNavVisibleLabel')")).toBeLessThan(
      prefs.indexOf("t('settings.general.pluginsNavVisibleLabel')"),
    );
  });

  it('marks routes and plugins toggles as in development', () => {
    const prefs = source('PreferencesPanel.tsx');
    expect(prefs).toContain("t('common.inDevelopment')");
    expect(prefs).toContain('badge={<Badge');
    expect((prefs.match(/t\('common\.inDevelopment'\)/g) ?? []).length).toBe(2);
    expect(prefs).toContain("aria-label={t('settings.general.routesNavVisibleLabel')}");
    expect(prefs).toContain("aria-label={t('settings.general.pluginsNavVisibleLabel')}");
  });
});
