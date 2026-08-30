import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('mcp layout wiring', () => {
  it('keeps the agent strip mounted while inventory loads so the table does not jump', () => {
    const page = source('index.tsx');
    expect(page).toContain('pageRhythm.chromeRow');
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).toContain('<AgentTabStrip');
    expect(page).toContain('TableSkeleton');
    expect(page).not.toContain('ListSkeleton');
    expect(page.indexOf('<AgentTabStrip')).toBeLessThan(page.indexOf('<TableSkeleton'));
    expect(page).toContain('agents={installedAgents}');
    expect(page).toContain('filterByPageVisibleAgent');
  });

  it('uses the shared 详情 toggle (label + chevron)', () => {
    const table = source('McpServerTable.tsx');
    expect(table).toContain('DetailsToggle');
    expect(table).toContain("t('mcp.table.details')");
    expect(table).not.toContain('h-6 px-1.5 text-xs');
  });

  it('says the page only scans and points at the existing folder button', () => {
    const page = source('index.tsx');
    expect(page).toContain("t('mcp.page.description')");
    expect(page).toContain("t('mcp.page.empty'");
    expect(page).toContain("t('mcp.page.nextStep')");
    expect(page).not.toContain('installMcp');
    expect(page).not.toContain('onInstall');
    expect(page).not.toContain('mcp.install');
    const table = source('McpServerTable.tsx');
    expect(table).toContain('<OpenDirButton');
    expect(table).toContain('labeled');
    expect(table).toContain('<CopyableFileName');
  });
});
