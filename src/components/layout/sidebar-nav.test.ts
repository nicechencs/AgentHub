import { describe, expect, it } from 'vitest';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import {
  filterManageNavItems,
  filterWorkspaceNavItems,
  manageNavItems,
  NAV_MANAGE,
  NAV_WORKSPACE,
  workspaceNavItems,
} from './sidebar-nav';

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
});
