import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Cloud, FolderCode, MessageSquare, Route } from 'lucide-react';
import { ROUTES_PATH, SUB2API_PATH } from '@/lib/routes-path';
import {
  ALL_NAV_HIDDEN,
  ALL_NAV_VISIBLE,
  DEFAULT_NAV_VISIBILITY,
  DEFAULT_PLUGINS_NAV_VISIBLE,
  DEFAULT_ROUTES_NAV_VISIBLE,
  DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES,
  DEFAULT_SUB2API_NAV_VISIBLE,
  navVisibilityWith,
  OPTIONAL_NAV_IDS,
} from '@/lib/ui-preferences';
import {
  ALWAYS_VISIBLE_NAV_PATHS,
  filterNavItems,
  manageNavItems,
  NAV_MANAGE,
  NAV_WORKSPACE,
  navItemInDevelopment,
  OPTIONAL_NAV_PATH,
  optionalNavIdForPath,
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

describe('filterNavItems', () => {
  it('keeps optional manage entries when visible', () => {
    expect(filterNavItems(MANAGE, ALL_NAV_VISIBLE).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      SUB2API_PATH,
      '/routes',
      '/settings',
    ]);
  });

  it('hides routes when not visible', () => {
    expect(
      filterNavItems(MANAGE, navVisibilityWith({ routes: false })).map((item) => item.to),
    ).toEqual(['/', '/connections', SUB2API_PATH, '/settings']);
  });

  it('hides Sub2API when preference is off', () => {
    expect(
      filterNavItems(MANAGE, navVisibilityWith({ sub2api: false })).map((item) => item.to),
    ).toEqual(['/', '/connections', '/routes', '/settings']);
  });

  it('hides connections when preference is off', () => {
    expect(
      filterNavItems(MANAGE, navVisibilityWith({ connections: false })).map((item) => item.to),
    ).toEqual(['/', SUB2API_PATH, '/routes', '/settings']);
  });

  it('keeps plugins after Projects when visible', () => {
    expect(filterNavItems(WORKSPACE, ALL_NAV_VISIBLE).map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
      '/plugins',
    ]);
  });

  it('hides plugins when not visible without renaming MCP', () => {
    expect(
      filterNavItems(WORKSPACE, navVisibilityWith({ plugins: false })).map((item) => item.to),
    ).toEqual(['/chat', '/agents', '/skills', '/mcp', '/projects']);
  });

  it('hides skills, MCP, and projects independently', () => {
    expect(
      filterNavItems(
        WORKSPACE,
        navVisibilityWith({ skills: false, mcp: false, projects: false }),
      ).map((item) => item.to),
    ).toEqual(['/chat', '/agents', '/plugins']);
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
    expect(sidebar).toContain('bg-accent-subtle font-medium text-primary [&_svg]:text-accent');
    expect(sidebar).toContain('hover:bg-hover hover:text-primary');
    expect(sidebar).toContain('const NAV_ICON_SIZE = 18;');
    expect(sidebar).toContain('size={NAV_ICON_SIZE}');
    expect(sidebar).toContain('strokeWidth={1.6}');
    expect(sidebar).toContain('data-icon="nav"');
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
  it('keeps chat, agents, dashboard, and settings when every optional entry is off', () => {
    expect(workspaceNavItems(ALL_NAV_HIDDEN).map((item) => item.to)).toEqual(['/chat', '/agents']);
    expect(manageNavItems(ALL_NAV_HIDDEN).map((item) => item.to)).toEqual(['/', '/settings']);
    expect([...ALWAYS_VISIBLE_NAV_PATHS]).toEqual(['/chat', '/agents', '/', '/settings']);
  });

  it('wraps workspace filter without changing paths', () => {
    expect(workspaceNavItems(ALL_NAV_VISIBLE).map((item) => item.to)).toEqual(
      filterNavItems(NAV_WORKSPACE, ALL_NAV_VISIBLE).map((item) => item.to),
    );
    expect(
      workspaceNavItems(navVisibilityWith({ plugins: false })).map((item) => item.to),
    ).toEqual(['/chat', '/agents', '/skills', '/mcp', '/projects']);
  });

  it('wraps manage filter and still hides routes only in the nav model', () => {
    expect(manageNavItems(ALL_NAV_VISIBLE).map((item) => item.to)).toEqual(
      filterNavItems(NAV_MANAGE, ALL_NAV_VISIBLE).map((item) => item.to),
    );
    expect(manageNavItems(navVisibilityWith({ routes: false })).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      SUB2API_PATH,
      '/settings',
    ]);
    expect(manageNavItems(navVisibilityWith({ sub2api: false })).map((item) => item.to)).toEqual([
      '/',
      '/connections',
      ROUTES_PATH,
      '/settings',
    ]);
  });

  it('maps every optional path and leaves always-visible paths unmapped', () => {
    expect(OPTIONAL_NAV_IDS.map((id) => OPTIONAL_NAV_PATH[id])).toEqual([
      '/skills',
      '/mcp',
      '/projects',
      '/plugins',
      '/connections',
      SUB2API_PATH,
      ROUTES_PATH,
    ]);
    for (const path of ALWAYS_VISIBLE_NAV_PATHS) {
      expect(optionalNavIdForPath(path)).toBeUndefined();
    }
    expect(optionalNavIdForPath('/skills')).toBe('skills');
    expect(optionalNavIdForPath(ROUTES_PATH)).toBe('routes');
  });

  it('shows routes by default and hides plugins in the sidebar for a new install', () => {
    expect(DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES).toBe(true);
    expect(DEFAULT_ROUTES_NAV_VISIBLE).toBe(true);
    expect(DEFAULT_PLUGINS_NAV_VISIBLE).toBe(false);
    expect(workspaceNavItems(DEFAULT_NAV_VISIBILITY).map((item) => item.to)).not.toContain(
      '/plugins',
    );
    expect(workspaceNavItems(DEFAULT_NAV_VISIBILITY).map((item) => item.to)).toEqual([
      '/chat',
      '/agents',
      '/skills',
      '/mcp',
      '/projects',
    ]);
    expect(manageNavItems(DEFAULT_NAV_VISIBILITY).map((item) => item.to)).toContain(ROUTES_PATH);
    expect(manageNavItems(DEFAULT_NAV_VISIBILITY).map((item) => item.to)).toContain(
      '/connections',
    );
    expect(manageNavItems(DEFAULT_NAV_VISIBILITY).map((item) => item.to)).not.toContain(
      SUB2API_PATH,
    );
    expect(DEFAULT_SUB2API_NAV_VISIBLE).toBe(false);
    const ctx = readFileSync(path.join(dir, 'SidebarContext.tsx'), 'utf8');
    expect(ctx).toContain(
      'loadBool(StorageKey.sidebarAutoCollapseOnRoutes, DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES)',
    );
    expect(ctx).toContain('OPTIONAL_NAV_STORAGE_KEY');
    expect(ctx).toContain('DEFAULT_NAV_VISIBILITY');
    expect(ctx).toContain('setNavVisible');
  });

  it('marks plugins as in development; MCP and routes are not', () => {
    const mcp = NAV_WORKSPACE.find((item) => item.to === '/mcp');
    const plugins = NAV_WORKSPACE.find((item) => item.to === '/plugins');
    const routes = NAV_MANAGE.find((item) => item.to === ROUTES_PATH);
    expect(mcp).toBeDefined();
    expect(plugins).toBeDefined();
    expect(routes).toBeDefined();
    expect(navItemInDevelopment(mcp!)).toBe(false);
    expect(navItemInDevelopment(plugins!)).toBe(true);
    expect(navItemInDevelopment(routes!)).toBe(false);
    expect(navItemInDevelopment(NAV_WORKSPACE[0])).toBe(false);
    expect(navItemInDevelopment(NAV_MANAGE[0])).toBe(false);
    const sidebar = readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
    expect(sidebar).toContain('navItemInDevelopment');
    expect(sidebar).toContain("t('common.inDevelopment')");
  });
});
