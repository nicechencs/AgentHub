import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { filterManageNavItems, filterWorkspaceNavItems } from './sidebar-nav';

const dir = path.dirname(fileURLToPath(import.meta.url));

function sidebarSource(): string {
  return readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
}

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

describe('workspace nav order', () => {
  it('places Plugins under Projects and keeps the MCP label', () => {
    const src = sidebarSource();
    const chat = src.indexOf("{ to: '/chat'");
    const agents = src.indexOf("{ to: '/agents'");
    const skills = src.indexOf("{ to: '/skills'");
    const mcp = src.indexOf("{ to: '/mcp', navKey: 'nav.mcp'");
    const projects = src.indexOf("{ to: '/projects'");
    const plugins = src.indexOf("{ to: '/plugins'");
    expect(chat).toBeGreaterThan(0);
    expect(agents).toBeGreaterThan(chat);
    expect(skills).toBeGreaterThan(agents);
    expect(mcp).toBeGreaterThan(skills);
    expect(projects).toBeGreaterThan(mcp);
    expect(plugins).toBeGreaterThan(projects);
    expect(src).toContain("navKey: 'nav.mcp'");
    expect(src).toContain("navKey: 'nav.plugins'");
  });
});
