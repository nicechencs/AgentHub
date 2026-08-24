import { describe, expect, it } from 'vitest';
import { filterManageNavItems } from './sidebar-nav';

const MANAGE = [
  { to: '/', navKey: 'nav.dashboard' },
  { to: '/connections', navKey: 'nav.connections' },
  { to: '/routes', navKey: 'nav.routes' },
  { to: '/settings', navKey: 'nav.settings' },
] as const;

describe('filterManageNavItems', () => {
  it('keeps routes when visible', () => {
    expect(filterManageNavItems(MANAGE, true).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      '/routes',
      '/settings',
    ]);
  });

  it('hides routes when not visible', () => {
    expect(filterManageNavItems(MANAGE, false).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      '/settings',
    ]);
  });
});
