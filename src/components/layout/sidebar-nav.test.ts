import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Cloud, FolderCode, MessageSquare, Route } from 'lucide-react';
import { ROUTES_PATH, SUB2API_PATH } from '@/lib/routes-path';
import {
  DEFAULT_PLUGINS_NAV_VISIBLE,
  DEFAULT_ROUTES_NAV_VISIBLE,
  DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES,
  DEFAULT_SUB2API_NAV_VISIBLE,
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
  { to: SUB2API_PATH, navKey: 'nav.sub2api' },
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
      SUB2API_PATH,
      '/routes',
      '/settings',
    ]);
  });

  it('hides routes when not visible', () => {
    expect(filterManageNavItems(MANAGE, false).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      SUB2API_PATH,
      '/settings',
    ]);
  });

  it('hides Sub2API when preference is off', () => {
    expect(filterManageNavItems(MANAGE, true, false).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      '/routes',
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

  it('keeps manage order: dashboard, connections, Sub2API, routes, settings', () => {
    expect(NAV_MANAGE.map((item) => item.to)).toEqual([
      '/',
      '/connections',
      SUB2API_PATH,
      ROUTES_PATH,
      '/settings',
    ]);
    expect(NAV_MANAGE.map((item) => item.navKey)).toEqual([
      'nav.dashboard',
      'nav.connections',
      'nav.sub2api',
      'nav.routes',
      'nav.settings',
    ]);
  });

  it('uses compact, recognizable icons for chat, projects, and routes', () => {
    expect(NAV_WORKSPACE.find((item) => item.to === '/chat')?.icon).toBe(MessageSquare);
    expect(NAV_WORKSPACE.find((item) => item.to === '/projects')?.icon).toBe(FolderCode);
    expect(NAV_MANAGE.find((item) => item.to === ROUTES_PATH)?.icon).toBe(Route);
    expect(NAV_MANAGE.find((item) => item.to === SUB2API_PATH)?.icon).toBe(Cloud);
  });

  it('keeps active labels readable while accenting 18px navigation icons', () => {
    const sidebar = readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
    expect(sidebar).toContain('bg-active font-medium text-primary [&_svg]:text-accent');
    expect(sidebar).toContain('hover:bg-hover hover:text-primary');
    expect(sidebar).toContain('const NAV_ICON_SIZE = 18;');
    expect(sidebar).toContain('size={NAV_ICON_SIZE}');
    expect(sidebar).toContain('strokeWidth={1.6}');
    expect(sidebar).toContain('absoluteStrokeWidth');
  });

  it('lets the expanded rail be dragged and remembers the width', () => {
    const sidebar = readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
    expect(sidebar).toContain('useSidebarWidth');
    expect(sidebar).toContain('NavResizeHandle');
    expect(sidebar).toContain("t('nav.resizeSidebar')");
    expect(sidebar).not.toContain("'w-56'");
    expect(sidebar).not.toContain('collapsed ? \'w-14\' : \'w-56\'');
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
    expect(manageNavItems(true, true).map((item) => item.to)).toEqual(
      filterManageNavItems(NAV_MANAGE, true, true).map((item) => item.to),
    );
    expect(manageNavItems(false, true).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      SUB2API_PATH,
      '/settings',
    ]);
    expect(manageNavItems(true, false).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      ROUTES_PATH,
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
    expect(
      manageNavItems(DEFAULT_ROUTES_NAV_VISIBLE, DEFAULT_SUB2API_NAV_VISIBLE).map(
        (item) => item.to,
      ),
    ).toContain(ROUTES_PATH);
    expect(
      manageNavItems(DEFAULT_ROUTES_NAV_VISIBLE, DEFAULT_SUB2API_NAV_VISIBLE).map(
        (item) => item.to,
      ),
    ).not.toContain(SUB2API_PATH);
    expect(DEFAULT_SUB2API_NAV_VISIBLE).toBe(false);
    const ctx = readFileSync(path.join(dir, 'SidebarContext.tsx'), 'utf8');
    expect(ctx).toContain(
      'loadBool(StorageKey.sidebarAutoCollapseOnRoutes, DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES)',
    );
    expect(ctx).toContain('loadBool(StorageKey.routesNavVisible, DEFAULT_ROUTES_NAV_VISIBLE)');
    expect(ctx).toContain('loadBool(StorageKey.pluginsNavVisible, DEFAULT_PLUGINS_NAV_VISIBLE)');
    expect(ctx).toContain('loadBool(StorageKey.sub2apiNavVisible, DEFAULT_SUB2API_NAV_VISIBLE)');
  });

  it('marks plugins and MCP as in development; routes are not', () => {
    const mcp = NAV_WORKSPACE.find((item) => item.to === '/mcp');
    const plugins = NAV_WORKSPACE.find((item) => item.to === '/plugins');
    const routes = NAV_MANAGE.find((item) => item.to === ROUTES_PATH);
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
