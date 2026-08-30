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

  it('keeps list as the exact-match index entry', () => {
    const list = ROUTES_NAV_ITEMS.find((item) => item.to === '/routes');
    expect(list?.end).toBe(true);
    expect(ROUTES_NAV_ITEMS.some((item) => item.end && item.to !== '/routes')).toBe(false);
  });

  it('flags pool and tokens as in development', () => {
    const byKey = Object.fromEntries(
      ROUTES_NAV_ITEMS.map((item) => [item.labelKey, routesNavItemInDevelopment(item)]),
    );
    expect(byKey['routes.nav.list']).toBe(false);
    expect(byKey['routes.nav.board']).toBe(false);
    expect(byKey['routes.nav.pool']).toBe(true);
    expect(byKey['routes.nav.tokens']).toBe(true);
    expect(byKey['routes.nav.activity']).toBe(false);
  });
});
