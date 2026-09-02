import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function sidebarSource(): string {
  return readFileSync(path.join(dir, 'Sidebar.tsx'), 'utf8');
}

/** ContextMenu JSX 块（锚定唯一的 open={railMenu 属性，避开 <ContextMenuPoint 等误匹配） */
function railMenuBlock(): string {
  const source = sidebarSource();
  const start = source.indexOf('<ContextMenu open={railMenu');
  const end = source.indexOf('</ContextMenu>', start);
  return source.slice(start, end);
}

describe('Sidebar rail right-click menu wiring', () => {
  it('opens the menu on right-click anywhere in the nav rail', () => {
    const source = sidebarSource();
    expect(source).toContain('onContextMenu={openRailMenu}');
    expect(source).toContain('e.preventDefault()');
  });

  it('offers exactly one action: expand when collapsed, collapse when expanded', () => {
    const menu = railMenuBlock();
    const expandBranch = menu.slice(menu.indexOf('collapsed ? ('), menu.indexOf(') : ('));
    const collapseBranch = menu.slice(menu.indexOf(') : ('), menu.indexOf('</ContextMenu>'));
    expect(expandBranch).toContain("t('nav.expandSidebar')");
    expect(expandBranch).toContain('expandFromRailMenu');
    expect(expandBranch).not.toContain('collapseFromRailMenu');
    expect(collapseBranch).toContain("t('nav.collapseSidebar')");
    expect(collapseBranch).toContain('collapseFromRailMenu');
    expect(collapseBranch).not.toContain('expandFromRailMenu');
  });

  it('menu actions drive collapsed state via setCollapsed', () => {
    const source = sidebarSource();
    expect(source).toContain('setCollapsed(false);');
    expect(source).toContain('setCollapsed(true);');
  });
});
