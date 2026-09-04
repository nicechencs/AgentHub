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
    expect(nav).toContain('bg-active font-medium text-primary [&_svg]:text-accent');
    expect(nav).toContain('hover:bg-hover hover:text-primary');
    expect(nav).toContain('const NAV_ICON_SIZE = 18;');
    expect(nav).toContain('size={NAV_ICON_SIZE}');
    expect(nav).toContain('strokeWidth={1.6}');
    expect(nav).toContain('absoluteStrokeWidth');
  });
});
