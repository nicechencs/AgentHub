import { describe, expect, it } from 'vitest';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import { collapsedAfterPrimaryNavClick } from './sidebar-collapse-override';

describe('collapsedAfterPrimaryNavClick', () => {
  it('collapses only the Routes item when the setting is on', () => {
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: BRIDGES_PATH,
        currentCollapsed: false,
        autoCollapseOnRoutes: true,
      }),
    ).toBe(true);
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: BRIDGES_PATH,
        currentCollapsed: true,
        autoCollapseOnRoutes: true,
      }),
    ).toBe(true);
  });

  it('does not change collapse for other primary items', () => {
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: '/chat',
        currentCollapsed: false,
        autoCollapseOnRoutes: true,
      }),
    ).toBe(false);
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: '/settings',
        currentCollapsed: true,
        autoCollapseOnRoutes: true,
      }),
    ).toBe(true);
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: '/',
        currentCollapsed: false,
        autoCollapseOnRoutes: true,
      }),
    ).toBe(false);
  });

  it('does not auto-collapse Routes when the setting is off', () => {
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: BRIDGES_PATH,
        currentCollapsed: false,
        autoCollapseOnRoutes: false,
      }),
    ).toBe(false);
    expect(
      collapsedAfterPrimaryNavClick({
        itemTo: BRIDGES_PATH,
        currentCollapsed: true,
        autoCollapseOnRoutes: false,
      }),
    ).toBe(true);
  });
});
