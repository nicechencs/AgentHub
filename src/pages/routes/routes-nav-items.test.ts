import { describe, expect, it } from 'vitest';
import {
  isRoutesAreaPath,
  ROUTES_NAV_ITEMS,
  routesNavItemInDevelopment,
} from './routes-nav-items';

describe('routes-nav-items', () => {
  it('marks /routes and nested paths as the routes area', () => {
    expect(isRoutesAreaPath('/routes')).toBe(true);
    expect(isRoutesAreaPath('/routes/board')).toBe(true);
    expect(isRoutesAreaPath('/connections')).toBe(false);
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
  });

  it('flags only tokens as in development', () => {
    const byKey = Object.fromEntries(
      ROUTES_NAV_ITEMS.map((item) => [item.labelKey, routesNavItemInDevelopment(item)]),
    );
    expect(byKey['routes.nav.board']).toBe(false);
    expect(byKey['routes.nav.pool']).toBe(false);
    expect(byKey['routes.nav.tokens']).toBe(true);
    expect(byKey['routes.nav.activity']).toBe(false);
  });
});
