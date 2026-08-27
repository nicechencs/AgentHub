import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { flattenKeys, translate } from '@/lib/i18n';
import { en } from '@/lib/i18n/locales/en';
import { zh } from '@/lib/i18n/locales/zh';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('connections layout wiring', () => {
  it('scopes tabs and wallet to installed agents after detect, omitting hidden and uninstalled', () => {
    const page = source('index.tsx');
    expect(page).toContain('filterWalletByExcludedAgents(wallet, omittedSet)');
    expect(page).toContain(
      'const allowedAgents = installedIds.length > 0 || !loading ? installedIds : visibleIds',
    );
    expect(page).toContain('const tabAgentIds = allowedAgents');
  });

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

  it('docks the recycle-bin button in the list column, left of the split', () => {
    const page = source('index.tsx');
    const button = source('ConnectionTrashButton.tsx');
    const split = source('../../components/layout/SideSplit.tsx');
    expect(page).toContain('listFooter={trashDock}');
    expect(page).toContain('<ConnectionTrashButton');
    expect(button).not.toContain('fixed bottom-4 right-4');
    expect(split).toContain('listFooter');
    expect(split).toContain('flex shrink-0 justify-end');
  });

  it('opens 详情 in the same right-hand inspect pane as edit', () => {
    const page = source('index.tsx');
    const list = source('TicketWalletList.tsx');
    expect(page).toContain("{ kind: 'detail'; ticketId: string }");
    expect(page).toContain('onShowDetail');
    expect(page).toContain('<TicketDetailPanel');
    expect(list).not.toContain('DetailsToggle');
    expect(list).toContain('asPanel');
    expect(list).toContain('{refreshButton}');
    expect(list).toContain('{deleteButton}');
    expect(list).not.toContain('footer={actions}');
    expect(list).toContain("t('connections.list.clientsTitle')");
    expect(list).toContain('SortHandle');
    expect(page).toContain('inspectActiveTicketId');
  });

  it('opens 用到其他工具 / 本机转发 in the same right-hand inspect pane', () => {
    const page = source('index.tsx');
    const shareRoute = source('use-connection-share-route.ts');
    expect(page).toContain("{ kind: 'connect'");
    expect(shareRoute).toContain("{ kind: 'connect'");
    expect(shareRoute).toContain("openConnectForTicket(ticket, 'share')");
    expect(shareRoute).toContain("openConnectForTicket(ticket, 'route')");
    expect(page).toContain('asPanel');
    expect(page).toContain('<ConnectFlowDialog');
    expect(page).not.toContain('const [connectEntry');
    expect(page).toContain("inspectTarget?.kind === 'connect'");
    expect(page).toContain('useConnectionShareRoute');
    expect(page).toContain('onShareTicket={handleShareTicket}');
    expect(page).toContain('onRouteTicket={handleRouteTicket}');
    const openerStart = shareRoute.indexOf('const openConnectForTicket');
    const openerEnd = shareRoute.indexOf('const handleShareTicket', openerStart);
    expect(openerStart).toBeGreaterThanOrEqual(0);
    expect(openerEnd).toBeGreaterThan(openerStart);
    expect(shareRoute.slice(openerStart, openerEnd)).not.toContain('inspect.close()');
    expect(shareRoute).not.toContain('inspect.close()');
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
    expect(form).toContain('configFieldUnsupported');
    expect(form).toContain('configFieldSecretPlaceholder');
    expect(form).not.toContain('field.help?.trim()');
    expect(form).not.toContain("connections.providerDialog.remoteModelsPick");
    expect(form).not.toContain("connections.providerDialog.remoteModelsCustom");
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

  it('labels official login and API Key as two kinds, not cryptic statuses', () => {
    const list = source('TicketWalletList.tsx');
    expect(list).toContain("t('kind.oauth')");
    expect(list).toContain("t('kind.apikey')");
    expect(list).not.toContain("t('connections.list.oauthAccount')");
    expect(list).not.toContain("t('connections.list.apiKeyAuth')");
    expect(translate('zh', 'kind.oauth')).toBe('官方登录');
    expect(translate('zh', 'kind.apikey')).toBe('API Key');
    expect(translate('en', 'kind.oauth')).toBe('Official login');
    expect(translate('en', 'kind.apikey')).toBe('API Key');
    expect(translate('zh', 'connections.list.oauthAccount')).toBe(translate('zh', 'kind.oauth'));
    expect(translate('zh', 'connections.list.apiKeyAuth')).toBe(translate('zh', 'kind.apikey'));
    expect(translate('en', 'connections.list.oauthAccount')).toBe(translate('en', 'kind.oauth'));
    expect(translate('en', 'connections.list.apiKeyAuth')).toBe(translate('en', 'kind.apikey'));
  });
});

const BANNED_UI = /票|钱包|投影|真源|PKCE|loopback|\bTicket\b|\bwallet\b|\bAdapter\b|\bwire/i;

function lookup(obj: unknown, key: string): string {
  let cur: unknown = obj;
  for (const part of key.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return key;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : key;
}

describe('connections user-facing copy', () => {
  it('keeps connections / dashboard / connect copy free of banned jargon', () => {
    const keys = flattenKeys(zh).filter((key) =>
      key.startsWith('connections.')
      || key.startsWith('dashboard.')
      || key.startsWith('connect.'),
    );
    expect(keys.length).toBeGreaterThan(50);
    for (const key of keys) {
      expect(lookup(zh, key), key).not.toMatch(BANNED_UI);
      expect(lookup(en, key), key).not.toMatch(BANNED_UI);
    }
  });
});
