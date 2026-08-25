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

  it('keeps live config chrome to the path and folder button; hint is hover-only', () => {
    const provider = source('../providers/ProviderEditDialog.tsx');
    expect(provider).toContain('Tip label={livePaths.hint}');
    expect(provider).toContain('isLiveFilePath(livePaths.auth)');
    expect(provider).not.toContain('<p className="text-muted">{livePaths.hint}</p>');
  });

  it('uses beginner field copy instead of schema help text', () => {
    const form = source('../../components/shared/GenericConfigForm.tsx');
    expect(form).toContain('configFieldHint');
    expect(form).toContain('configFieldLabel');
    expect(form).toContain('configFieldOptionLabel');
    expect(form).not.toContain('field.help?.trim()');
  });

  it('keeps key hints on the key field and shows cancel in the inspect header', () => {
    const provider = source('../providers/ProviderEditDialog.tsx');
    const account = source('../accounts/ApiKeyAccountDialog.tsx');
    expect(provider).toContain("t('connections.apiKeyDialog.keyHint')");
    expect(provider).toContain('fieldHints');
    expect(provider).toContain("t('common.cancel')");
    expect(provider).toContain('variant="secondary"');
    expect(provider).toContain('headerActions={headerActions}');
    expect(account).toContain("t('connections.apiKeyDialog.keyHint')");
    expect(account).toContain("t('common.cancel')");
    expect(account).toContain('variant="secondary"');
    expect(account).toContain('headerActions={headerActions}');
  });

  it('clears the guided-add marker when the provider pane is dismissed', () => {
    const page = source('index.tsx').replace(/\r\n/g, '\n');
    expect(page).toContain(
      'guideOpenedApiKeyRef.current = false;\n            inspect.close();',
    );
  });
});
