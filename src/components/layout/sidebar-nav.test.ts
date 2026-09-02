import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import {
  DEFAULT_PLUGINS_NAV_VISIBLE,
  DEFAULT_ROUTES_NAV_VISIBLE,
  DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES,
} from '@/lib/ui-preferences';
import {
  filterManageNavItems,
  filterWorkspaceNavItems,
  manageNavItems,
  NAV_MANAGE,
  NAV_WORKSPACE,
  navItemInDevelopment,
  workspaceNavItems,
} from './sidebar-nav';

const dir = path.dirname(fileURLToPath(import.meta.url));

const MANAGE = [
  { to: '/', navKey: 'nav.dashboard' },
  { to: '/connections', navKey: 'nav.connections' },
  { to: '/routes', navKey: 'nav.routes' },
  { to: '/settings', navKey: 'nav.settings' },
] as const;

const WORKSPACE = [
  { to: '/chat', navKey: 'nav.chat' },
  { to: '/agents', navKey: 'nav.agents' },
  { to: '/skills', navKey: 'nav.skills' },
  { to: '/mcp', navKey: 'nav.mcp' },
  { to: '/projects', navKey: 'nav.projects' },
  { to: '/plugins', navKey: 'nav.plugins' },
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

describe('filterWorkspaceNavItems', () => {
  it('keeps plugins after Projects when visible', () => {
    expect(filterWorkspaceNavItems(WORKSPACE, true).map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
      '/plugins',
    ]);
  });

  it('hides plugins when not visible without renaming MCP', () => {
    expect(filterWorkspaceNavItems(WORKSPACE, false).map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
    ]);
  });
});

describe('nav model order', () => {
  it('places Plugins under Projects and keeps the MCP label', () => {
    expect(NAV_WORKSPACE.map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
      '/plugins',
    ]);
    expect(NAV_WORKSPACE.map((item) => item.navKey)).toEqual([
      'nav.chat',
      'nav.agents',
      'nav.skills',
      'nav.mcp',
      'nav.projects',
      'nav.plugins',
    ]);
  });

  it('keeps manage order: dashboard, connections, routes, settings', () => {
    expect(NAV_MANAGE.map((item) => item.to)).toEqual([
      '/',
      '/connections',
      BRIDGES_PATH,
      '/settings',
    ]);
    expect(NAV_MANAGE.map((item) => item.navKey)).toEqual([
      'nav.dashboard',
      'nav.connections',
      'nav.routes',
      'nav.settings',
    ]);
  });
});

describe('workspaceNavItems / manageNavItems', () => {
  it('wraps workspace filter without changing paths', () => {
    expect(workspaceNavItems(true).map((item) => item.to)).toEqual(
      filterWorkspaceNavItems(NAV_WORKSPACE, true).map((item) => item.to),
    );
    expect(workspaceNavItems(false).map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
    ]);
  });

  it('wraps manage filter and still hides routes only in the nav model', () => {
    expect(manageNavItems(true).map((item) => item.to)).toEqual(
      filterManageNavItems(NAV_MANAGE, true).map((item) => item.to),
    );
    expect(manageNavItems(false).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      '/settings',
    ]);
  });

  it('shows routes by default and hides plugins in the sidebar for a new install', () => {
    expect(DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES).toBe(true);
    expect(DEFAULT_ROUTES_NAV_VISIBLE).toBe(true);
    expect(DEFAULT_PLUGINS_NAV_VISIBLE).toBe(false);
    expect(workspaceNavItems(DEFAULT_PLUGINS_NAV_VISIBLE).map((item) => item.to)).not.toContain(
      '/plugins',
    );
    expect(manageNavItems(DEFAULT_ROUTES_NAV_VISIBLE).map((item) => item.to)).toContain(
      BRIDGES_PATH,
    );
    const ctx = readFileSync(path.join(dir, 'SidebarContext.tsx'), 'utf8');
    expect(ctx).toContain(
      'loadBool(StorageKey.sidebarAutoCollapseOnRoutes, DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES)',
    );
    expect(ctx).toContain('loadBool(StorageKey.routesNavVisible, DEFAULT_ROUTES_NAV_VISIBLE)');
    expect(ctx).toContain('loadBool(StorageKey.pluginsNavVisible, DEFAULT_PLUGINS_NAV_VISIBLE)');
  });

  it('marks plugins and MCP as in development; routes are not', () => {
    const mcp = NAV_WORKSPACE.find((item) => item.to === '/mcp');
    const plugins = NAV_WORKSPACE.find((item) => item.to === '/plugins');
    const routes = NAV_MANAGE.find((item) => item.to === BRIDGES_PATH);
    expect(mcp).toBeDefined();
    expect(plugins).toBeDefined();
    expect(routes).toBeDefined();
    expect(navItemInDevelopment(mcp!)).toBe(true);
    expect(navItemInDevelopment(plugins!)).toBe(true);
    expect(navItemInDevelopment(routes!)).toBe(false);
    expect(navItemInDevelopment(NAV_WORKSPACE[0])).toBe(false);
    expect(navItemInDevelopment(NAV_MANAGE[0])).toBe(false);
    const sidebar = readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
    expect(sidebar).toContain('navItemInDevelopment');
    expect(sidebar).toContain("t('common.inDevelopment')");
  });
});
