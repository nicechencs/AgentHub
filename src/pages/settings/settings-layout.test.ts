import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('settings layout wiring', () => {
  it('keeps the page title on the edge column; only the body uses the reading column', () => {
    const page = source('index.tsx');
    expect(page).toContain('pageRhythm.readingStart');
    expect(page).not.toContain('pageRhythm.formColumn');
    expect(page.match(/className=\{pageRhythm\.readingStart\}/g)?.length).toBe(4);
    expect(page).not.toMatch(/readingColumn\}>\s*<PageHeader/);
    expect(page).toContain('TabsContent value="backups" className="h-full min-h-0"');
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
});
