import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  NAV_ICON_SIZE,
  NAV_ICON_STROKE,
  navBrandTitleClass,
  navHeaderClass,
  navItemClass,
  navListClass,
  navToggleClass,
} from './nav-chrome';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('nav chrome tokens', () => {
  it('keeps one selected and hover language for expanded and collapsed rails', () => {
    const expanded = navItemClass(true, false);
    const collapsed = navItemClass(true, true);
    const idle = navItemClass(false, false);
    expect(expanded).toContain('bg-accent-subtle font-medium text-primary [&_svg]:text-accent');
    expect(expanded).toContain('before:bg-accent');
    expect(collapsed).toContain('bg-accent-subtle font-medium text-primary [&_svg]:text-accent');
    expect(collapsed).not.toContain('before:bg-accent');
    expect(collapsed).toContain('justify-center');
    expect(idle).toContain('text-secondary hover:bg-hover hover:text-primary');
    expect(NAV_ICON_SIZE).toBe(18);
    expect(NAV_ICON_STROKE).toBe(1.6);
    expect(navListClass).toContain('gap-0.5');
    expect(navToggleClass).toContain('hover:bg-hover hover:text-primary');
    expect(navBrandTitleClass).toContain('text-body font-semibold');
    expect(navHeaderClass(true)).toContain('justify-center');
    expect(navHeaderClass(false)).toContain('justify-between px-3');
  });

  it('wires the primary sidebar and Routes rail through the same chrome', () => {
    const sidebar = source('Sidebar.tsx');
    const routes = readFileSync(path.join(dir, '../../pages/routes/RoutesNav.tsx'), 'utf8');
    const header = source('NavRailHeader.tsx');
    expect(sidebar).toContain('navItemClass');
    expect(sidebar).toContain('NavRailHeader');
    expect(sidebar).toContain('navListClass');
    expect(routes).toContain('navItemClass');
    expect(routes).toContain('NavRailHeader');
    expect(routes).toContain('navListClass');
    expect(header).toContain('navHeaderClass');
    expect(header).toContain('navToggleClass');
    expect(header).toContain('group-hover:opacity-0');
    expect(sidebar).not.toContain('const NAV_ICON_SIZE');
    expect(routes).not.toContain('const NAV_ICON_SIZE');
    expect(routes).not.toContain('pageRhythm.pageTitle');
  });
});
