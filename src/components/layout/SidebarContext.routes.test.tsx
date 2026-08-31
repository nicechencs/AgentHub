import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('SidebarContext routes-area wiring', () => {
  it('persists collapse and the auto-collapse setting; no session override', () => {
    const ctx = source('SidebarContext.tsx');
    expect(ctx).toContain('StorageKey.sidebarCollapsed');
    expect(ctx).toContain('StorageKey.sidebarAutoCollapseOnRoutes');
    expect(ctx).toContain('DEFAULT_SIDEBAR_AUTO_COLLAPSE_ON_ROUTES');
    expect(ctx).toContain('saveBool(StorageKey.sidebarCollapsed');
    expect(ctx).toContain('saveBool(StorageKey.sidebarAutoCollapseOnRoutes');
    expect(ctx).not.toContain('onEnterRoutesArea');
    expect(ctx).not.toContain('onLeaveRoutesArea');
    expect(ctx).not.toContain('sessionCollapsed');
    expect(ctx).toContain('expandPrimarySidebar');
  });

  it('RoutesLayout does not auto-expand or auto-collapse the primary sidebar', () => {
    const layout = readFileSync(
      path.join(dir, '../../pages/routes/RoutesLayout.tsx'),
      'utf8',
    );
    expect(layout).not.toContain('enterRoutesArea');
    expect(layout).not.toContain('leaveRoutesArea');
    expect(layout).not.toContain('useLayoutEffect');
    expect(layout).not.toContain('useSidebar');
    expect(layout).toContain('data-routes-layout');
  });

  it('primary Routes click uses the collapse policy', () => {
    const sidebar = source('Sidebar.tsx');
    expect(sidebar).toContain('collapsedAfterPrimaryNavClick');
    expect(sidebar).toContain('autoCollapseOnRoutes');
  });
});
