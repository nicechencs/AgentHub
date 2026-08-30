import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('SidebarContext routes-area wiring', () => {
  it('uses session collapse helpers and never writes storage on enter/expand/leave', () => {
    const ctx = source('SidebarContext.tsx');
    expect(ctx).toContain('onEnterRoutesArea');
    expect(ctx).toContain('onExpandPrimaryFromRoutes');
    expect(ctx).toContain('onLeaveRoutesArea');
    expect(ctx).toContain('onToggleInRoutesArea');
    expect(ctx).toContain('effectiveCollapsed');
    // enter / expand / leave must not call saveBool for sidebarCollapsed
    const enterBlock = ctx.slice(ctx.indexOf('enterRoutesArea'), ctx.indexOf('leaveRoutesArea'));
    expect(enterBlock).not.toContain('StorageKey.sidebarCollapsed');
    const expandBlock = ctx.slice(
      ctx.indexOf('expandPrimarySidebar'),
      ctx.indexOf('routesNavVisible,'),
    );
    expect(expandBlock).not.toContain('saveBool(StorageKey.sidebarCollapsed');
  });

  it('RoutesLayout applies collapse in useLayoutEffect', () => {
    const layout = readFileSync(
      path.join(dir, '../../pages/routes/RoutesLayout.tsx'),
      'utf8',
    );
    expect(layout).toContain('useLayoutEffect');
    expect(layout).toContain('enterRoutesArea');
    expect(layout).toContain('leaveRoutesArea');
  });
});
