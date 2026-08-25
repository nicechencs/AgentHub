import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('connections layout wiring', () => {
  it('uses leftover-inactive filtered length for chips and footer; header descriptionCount stays unfiltered', () => {
    const page = source('index.tsx');
    const list = source('TicketWalletList.tsx');

    expect(page).toContain(
      "counts[id] = filterTicketsByAgentUsage(visibleWallet, tickets, id).length",
    );
    expect(list).toContain("t('connections.list.count', { n: rows.length })");

    // Intentional: page subtitle counts the whole wallet, not the Agent-tab filter.
    expect(page).toContain(
      "t('connections.page.descriptionCount', { n: visibleWallet.tickets.length })",
    );
    expect(page).not.toContain(
      "t('connections.page.descriptionCount', { n: filterTicketsByAgentUsage",
    );
  });

  it('opens edit/add as a resizable workbench inspect pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain("size=\"compact\"");
    expect(page).toContain("t('common.resizeSidePanel')");
    expect(page).toContain('asPanel');
    expect(page).not.toContain('<Dialog open={apiKeyDialogOpen}');
  });
});
