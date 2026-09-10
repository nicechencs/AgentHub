import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Network } from 'lucide-react';
import {
  isRoutesAreaPath,
  ROUTES_NAV_ITEMS,
  routesNavItemInDevelopment,
} from './routes-nav-items';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('routes-nav-items', () => {
  it('marks /routes and nested paths as the routes area', () => {
    expect(isRoutesAreaPath('/routes')).toBe(true);
    expect(isRoutesAreaPath('/routes/board')).toBe(true);
    expect(isRoutesAreaPath('/connections')).toBe(false);
    expect(isRoutesAreaPath('/sub2api')).toBe(false);
    expect(isRoutesAreaPath('/routes-extra')).toBe(false);
  });

  it('does not keep a route-list nav entry; each page has its own path', () => {
    expect(ROUTES_NAV_ITEMS.some((item) => item.labelKey === 'routes.nav.list')).toBe(false);
    expect(ROUTES_NAV_ITEMS.map((item) => item.to)).toEqual([
      '/routes/board',
      '/routes/pool',
      '/routes/tokens',
      '/routes/activity',
    ]);
    expect(ROUTES_NAV_ITEMS.map((item) => item.to)).not.toContain('/routes/sub2api');
    expect(ROUTES_NAV_ITEMS.map((item) => item.to)).not.toContain('/sub2api');
  });

  it('does not mark any routes sub-nav item as in development', () => {
    expect(ROUTES_NAV_ITEMS.every((item) => !routesNavItemInDevelopment(item))).toBe(true);
  });

  it('does not keep Sub2API in the routes secondary nav', () => {
    expect(ROUTES_NAV_ITEMS.some((item) => item.labelKey === 'routes.nav.sub2api')).toBe(false);
  });

  it('uses a network icon for the connection pool', () => {
    expect(ROUTES_NAV_ITEMS.find((item) => item.to === '/routes/pool')?.icon).toBe(Network);
  });

  it('keeps secondary-nav labels readable while accenting 18px icons', () => {
    const nav = readFileSync(path.join(dir, 'RoutesNav.tsx'), 'utf8');
    expect(nav).toContain('navItemClass');
    expect(nav).toContain('size={NAV_ICON_SIZE}');
    expect(nav).toContain('strokeWidth={NAV_ICON_STROKE}');
    expect(nav).toContain('data-icon="nav"');
    expect(nav).toContain('absoluteStrokeWidth');
  });

  it('lets the expanded secondary rail be dragged and remembers the width', () => {
    const nav = readFileSync(path.join(dir, 'RoutesNav.tsx'), 'utf8');
    expect(nav).toContain('useNavWidth');
    expect(nav).toContain('ROUTES_NAV_WIDTH');
    expect(nav).toContain('StorageKey.routesNavWidth');
    expect(nav).toContain('NavResizeHandle');
    expect(nav).toContain("t('routes.nav.resize')");
    expect(nav).not.toContain('w-12 lg:w-48');
  });

  it('collapses the secondary rail from a top-right control', () => {
    const nav = readFileSync(path.join(dir, 'RoutesNav.tsx'), 'utf8');
    expect(nav).toContain('StorageKey.routesNavCollapsed');
    expect(nav).toContain("t('routes.nav.collapse')");
    expect(nav).toContain("t('routes.nav.expand')");
    expect(nav).toContain('NavRailHeader');
    const header = readFileSync(path.join(dir, '../../components/layout/NavRailHeader.tsx'), 'utf8');
    expect(header).toContain('RailCollapseIcon');
    expect(header).toContain('RailExpandIcon');
    expect(header).toContain('group-hover:opacity-0');
    expect(nav).toContain('<Route');
    expect(nav.indexOf('<Route')).toBeLessThan(nav.indexOf("t('routes.nav.title')"));
    expect(nav).not.toContain('text-sm font-semibold tracking-tight');
    expect(nav).not.toContain('pageRhythm.pageTitle');
    expect(nav.indexOf("t('routes.nav.title')")).toBeLessThan(nav.indexOf("t('routes.nav.collapse')"));
    expect(nav).not.toContain('expandPrimarySidebar');
    expect(nav).not.toContain("t('nav.expandSidebar')");
    expect(nav).not.toContain('useSidebar');
  });

  it('opens a right-click menu on the secondary rail with one expand or collapse action', () => {
    const nav = readFileSync(path.join(dir, 'RoutesNav.tsx'), 'utf8');
    expect(nav).toContain('onContextMenu={openRailMenu}');
    expect(nav).toContain('e.preventDefault()');
    const start = nav.indexOf('<ContextMenu open={railMenu');
    const end = nav.indexOf('</ContextMenu>', start);
    const menu = nav.slice(start, end);
    const expandBranch = menu.slice(menu.indexOf('collapsed ? ('), menu.indexOf(') : ('));
    const collapseBranch = menu.slice(menu.indexOf(') : ('), menu.length);
    expect(expandBranch).toContain("t('routes.nav.expand')");
    expect(expandBranch).toContain('expandFromRailMenu');
    expect(expandBranch).not.toContain('collapseFromRailMenu');
    expect(collapseBranch).toContain("t('routes.nav.collapse')");
    expect(collapseBranch).toContain('collapseFromRailMenu');
    expect(collapseBranch).not.toContain('expandFromRailMenu');
    expect(nav).toContain('setRailCollapsed(false)');
    expect(nav).toContain('setRailCollapsed(true)');
  });
});
