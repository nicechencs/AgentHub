import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('agents layout wiring', () => {
  it('drag-reorders Agent cards and remembers the catalog order', () => {
    const page = source('pages/agents/index.tsx');
    expect(page).toContain('SortHandle');
    expect(page).toContain('useSortableDrag');
    expect(page).toContain('onDragStartId');
    expect(page).toContain('useStoredIdOrder');
    expect(page).toContain('StorageKey.agentsCatalogOrder');
    expect(page).toContain('applyStoredAgentOrder');
    expect(page).toContain('sortHandle=');
    const card = source('pages/agents/agent-card.tsx');
    expect(card).toContain('sortHandle?:');
    expect(card).toContain('{sortHandle}');
  });

  it('applies the same remembered order on sidebar, dashboard, and installed lists', () => {
    const sidebar = source('components/layout/Sidebar.tsx');
    expect(sidebar).toContain('StorageKey.agentsCatalogOrder');
    expect(sidebar).toContain('applyStoredAgentOrder');
    const overview = source('pages/dashboard/AgentOverview.tsx');
    expect(overview).toContain('StorageKey.agentsCatalogOrder');
    const dashboard = source('pages/dashboard/index.tsx');
    expect(dashboard).toContain('installedAgents');
    expect(dashboard).toContain('omittedIds');
    const hook = source('lib/hooks/useInstalledAgents.ts');
    expect(hook).toContain('StorageKey.agentsCatalogOrder');
    const prefs = source('lib/ui-preferences.ts');
    expect(prefs).toContain('agentsCatalogOrder');
    expect(prefs).toContain('connectionsTicketOrder');
    expect(prefs).toContain('routesProfileOrder');
  });
});
