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
    expect(page).toContain('useOAuthLoginAgents(manageAuthAgentIds)');
    expect(page).toContain('buildTicketAddMenu(manageAuthAgentIds, oauthLoginAgents)');
    expect(page).toContain('oauthLoginAgents={oauthLoginAgents}');
    expect(page).toContain('disabled={authBlockedIds}');
    expect(page).toContain("t('connections.capability.authUnsupported')");
    expect(page).toContain('isAuthorizationManagementBlocked');
  });

  it('uses leftover-inactive filtered length for chips and footer; header descriptionCount stays unfiltered', () => {
    const page = source('index.tsx');
    const list = source('TicketWalletList.tsx');

    expect(page).toContain(
      'counts[id] = tickets.filter((ticket) => ticket.agentId === id).length',
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
    expect(page).toContain('PageHeader');
    expect(page).toContain('pageRhythm.chromeActions');
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
    expect(page).toContain('onChanged={handleTrashChanged}');
    expect(page).toContain('void Promise.all([loadWallet(), poolReload().catch(() => {})])');
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
    expect(list).toContain('data-ticket-name');
    expect(list).not.toContain('onOpen={onShowDetail');
    expect(list).not.toContain("t('connections.list.details')");
    expect(list).toContain('TableShell');
    expect(list).toContain('TableRow');
    expect(list).toContain('AgentLogo');
    expect(list).not.toContain('ListRowBody');
    expect(list).not.toContain('LIST_ROW_PAD');
    expect(list).not.toContain('DetailsToggle');
    expect(list).toContain('asPanel');
    expect(list).toContain('{refreshButton}');
    expect(list).toContain('{deleteButton}');
    expect(list).not.toContain('footer={actions}');
    expect(list).toContain("t('connections.list.clientsTitle')");
    expect(list).toContain('SortHandle');
    expect(page).toContain('inspectActiveTicketId');
  });

  it('imports a login to the connection pool from the row menu', () => {
    const page = source('index.tsx');
    const list = source('TicketWalletList.tsx');
    expect(page).toContain('useTicketPoolImport');
    expect(page).toContain('onImportToPool=');
    expect(page).toContain('onRemoveFromCatalog=');
    expect(page).toContain('importActionForTicket={importActionForTicket}');
    expect(page).toContain('importingTicketId={importingTicketId}');
    expect(page).not.toContain('useConnectionShareRoute');
    expect(page).not.toContain('onShareTicket');
    expect(page).not.toContain('onRouteTicket');
    expect(page).not.toContain('<ConnectFlowDialog');
    expect(page).not.toContain("{ kind: 'connect'");
    expect(list).toContain('onContextMenu=');
    expect(list).toContain('<ContextMenu');
    expect(list).toContain("t('connections.list.importToPool')");
    expect(list).toContain("t('connections.list.removeFromCatalog')");
    expect(list).toContain('<Share2');
    expect(list).not.toContain('<Import');
    expect(list).not.toContain("t('connections.list.share')");
    expect(list).not.toContain("t('connections.list.route')");
  });

  it('opens ticket detail without toggling closed on a second click of the same card', () => {
    const page = source('index.tsx');
    const start = page.indexOf('const handleShowDetail');
    const end = page.indexOf('const importCoexistenceNotice', start);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const block = page.slice(start, end);
    expect(block).toContain("inspect.open({ kind: 'detail', ticketId: ticket.id })");
    expect(block).not.toContain('inspect.close()');
  });

  it('keeps live config chrome to the path and folder button; hint is hover-only', () => {
    const provider = source('../../components/connections/ProviderEditDialog.tsx');
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
    const provider = source('../../components/connections/ProviderEditDialog.tsx');
    const account = source('../../components/connections/ApiKeyAccountDialog.tsx');
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

  it('adds a WorkBuddy API Key as a catalog login, not a replacing provider snapshot', () => {
    const page = source('index.tsx');
    expect(page).toContain("next.addAgentId === 'workbuddy'");
    expect(page).toContain("kind: 'account'");
    expect(page).toContain('account: null');
    expect(page).toContain("mode={inspectTarget.account ? 'edit' : 'add'}");
  });

  it('clears the guided-add marker when the provider pane is dismissed', () => {
    const page = source('index.tsx').replace(/\r\n/g, '\n');
    expect(page).toContain(
      'guideOpenedApiKeyRef.current = false;\n            setApiKeyDraft(null);\n            inspect.close();',
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
