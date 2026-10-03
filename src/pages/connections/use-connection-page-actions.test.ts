import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import {
  deleteConnectionTicket,
  describeProviderSwitchError,
  describePiLiveDeleteError,
  describePiProviderActionError,
  removeCatalogTicket,
  SWITCH_WROTE_LIVE,
  switchErrorText,
  switchWroteLiveLabel,
  useConnectionPageActions,
  type ConnectionActionApi,
} from './use-connection-page-actions';

const api = vi.hoisted(() => ({
  disconnectPiProvider: vi.fn(),
  undoSwitchAccount: vi.fn(),
  undoSwitch: vi.fn(),
  deleteAccount: vi.fn(),
  deleteProvider: vi.fn(),
  switchAccount: vi.fn(),
  switchPreview: vi.fn(),
  switchProvider: vi.fn(),
  bindTicket: vi.fn(),
  logGuiEvent: vi.fn(),
}));

vi.mock('@/lib/api/provider', () => ({
  deleteProvider: api.deleteProvider,
  disconnectPiProvider: api.disconnectPiProvider,
  switchPreview: api.switchPreview,
  switchProvider: api.switchProvider,
  undoSwitch: api.undoSwitch,
}));
vi.mock('@/lib/api/account', () => ({
  deleteAccount: api.deleteAccount,
  switchAccount: api.switchAccount,
  undoSwitchAccount: api.undoSwitchAccount,
}));
vi.mock('@/lib/api/settings', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api/settings')>()),
  logGuiEvent: api.logGuiEvent,
}));

const tZh = createTranslator('zh');
const tEn = createTranslator('en');

function ticket(overrides: Partial<TicketView> = {}): TicketView {
  return {
    id: 'provider:pi-current',
    sourceKind: 'provider',
    sourceId: 'pi-current',
    agentId: 'pi',
    label: 'Pi OpenAI',
    surface: 'openai-api',
    credentialClass: 'api_key',
    speaks: ['openai-responses'],
    importedFrom: 'pi',
    ...overrides,
  };
}

function actionApi(): ConnectionActionApi {
  return {
    disconnectPiProvider: api.disconnectPiProvider,
    undoSwitchAccount: api.undoSwitchAccount,
    undoSwitch: api.undoSwitch,
    deleteAccount: api.deleteAccount,
    deleteProvider: api.deleteProvider,
  };
}

function mountActions(input: Parameters<typeof useConnectionPageActions>[0]): ReturnType<typeof useConnectionPageActions> {
  let actions: ReturnType<typeof useConnectionPageActions> | null = null;
  function Harness() {
    actions = useConnectionPageActions(input);
    return null;
  }
  renderToStaticMarkup(createElement(Harness));
  return actions!;
}

describe('describeProviderSwitchError', () => {
  it('maps Cursor unsupported / rollback to a localized live-write failure (zh)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
      tZh,
    )).toBe(
      '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录。',
    );
    expect(describeProviderSwitchError('cursor', new Error('unsupported'), tZh)).toContain('请用 Cursor 自己的登录');
    expect(describeProviderSwitchError('cursor', { message: 'unsupported' }, tZh)).toContain('未能写入本机配置');
  });

  it('maps Cursor unsupported / rollback to a localized live-write failure (en)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
      tEn,
    )).toBe(
      "Failed to write local config. Cursor can't write this login to its local config yet. Use Cursor's own sign-in.",
    );
    expect(describeProviderSwitchError('cursor', new Error('unsupported'), tEn)).toContain("Use Cursor's own sign-in");
    expect(describeProviderSwitchError('cursor', { message: 'unsupported' }, tEn)).toContain('Failed to write local config');
  });

  it('falls back to the Chinese default when no translator is passed (backward compat)', () => {
    expect(describeProviderSwitchError(
      'cursor',
      'provider switch failed [unsupported]; compensation status: live=unsupported, database=ok [provider.switch.rollback]',
    )).toBe(
      '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录。',
    );
  });

  it('does not swallow a non-unsupported Cursor failure', () => {
    expect(describeProviderSwitchError('cursor', new Error('provider not found: missing'), tZh))
      .toBe('provider not found: missing');
    expect(describeProviderSwitchError('cursor', new Error('provider not found: missing'), tEn))
      .toBe('provider not found: missing');
  });

  it('keeps other agents\' error text', () => {
    expect(describeProviderSwitchError('claude', 'IO error: disk full [io]', tZh))
      .toBe('IO error: disk full');
    expect(describeProviderSwitchError('claude', 'IO error: disk full [io]', tEn))
      .toBe('IO error: disk full');
  });

  it('uses the localized failed-to-write fallback when the payload has no message', () => {
    expect(describeProviderSwitchError('claude', {}, tZh)).toBe('未能写入本机配置');
    expect(describeProviderSwitchError('claude', {}, tEn)).toBe('Failed to write local config');
    expect(describeProviderSwitchError('claude', {})).toBe('未能写入本机配置');
    expect(switchErrorText({})).toBe('');
  });
});

describe('switch toast copy', () => {
  it('names a successful live write (Chinese fallback constant, backward compat)', () => {
    expect(SWITCH_WROTE_LIVE).toBe('已写入本机配置');
  });

  it('switchWroteLiveLabel translates via t and falls back without one', () => {
    expect(switchWroteLiveLabel(tZh)).toBe('已写入本机配置');
    expect(switchWroteLiveLabel(tEn)).toBe('Wrote to local config');
    expect(switchWroteLiveLabel()).toBe('已写入本机配置');
    expect(switchWroteLiveLabel(tZh, 'catalogAppend')).toBe('已写入模型列表');
    expect(switchWroteLiveLabel(tEn, 'catalogAppend')).toBe('Wrote to the model list');
    expect(switchWroteLiveLabel(undefined, 'catalogAppend')).toBe('已写入模型列表');
  });

  it('keeps the wrote-live label off the bind-to-route success path', () => {
    const src = readFileSync(new URL('./use-connection-page-actions.ts', import.meta.url), 'utf8');
    expect(src).toContain('switchWroteLiveLabel(t, resolveAgentMeta(ticket.agentId).occupancy)');
    expect(src).toMatch(/const wroteLocal =\s*ticket\.agentId === targetAgent/);
    expect(src).toContain('extras?.isCurrent');
    expect(src).toContain('extras?.inList');
    expect(src).toContain("t('connections.list.switchDefaultOk')");
    expect(src).not.toContain('tabCurrentId');
  });

  it('cancels catalog add by undoing the last write to that tool', () => {
    const src = readFileSync(new URL('./use-connection-page-actions.ts', import.meta.url), 'utf8');
    expect(src).toContain('handleRemoveFromCatalog');
    expect(src).toContain('undoSwitchAccount(ticket.agentId)');
    expect(src).toContain('undoSwitch(ticket.agentId)');
    expect(src).toContain("t('connections.list.removeFromCatalogOk')");
    expect(src).toContain("t('connections.list.removeFromCatalogFail')");
  });

  it('cancels a current Pi provider through its concrete row without undo', async () => {
    vi.resetAllMocks();
    api.disconnectPiProvider.mockResolvedValue(undefined);
    const current = ticket();

    await expect(removeCatalogTicket(current, { isCurrent: true, inList: true }, actionApi()))
      .resolves.toBe(true);
    expect(api.disconnectPiProvider).toHaveBeenCalledWith('pi-current', false);
    expect(api.undoSwitch).not.toHaveBeenCalled();
    expect(api.undoSwitchAccount).not.toHaveBeenCalled();
  });

  it('deletes a current Pi official login through the account path, not cancel-connect', async () => {
    vi.resetAllMocks();
    api.deleteAccount.mockResolvedValue(undefined);
    const current = ticket({
      id: 'account:pi-oauth',
      sourceKind: 'account',
      sourceId: 'pi-oauth',
      credentialClass: 'oauth',
    });

    await deleteConnectionTicket(current, { isCurrent: true }, actionApi());

    expect(api.deleteAccount).toHaveBeenCalledWith('pi', 'pi-oauth');
    expect(api.disconnectPiProvider).not.toHaveBeenCalled();
    expect(api.deleteProvider).not.toHaveBeenCalled();
  });

  it('deletes a current Pi provider from live config and an old Pi row from the pool only', async () => {
    vi.resetAllMocks();
    api.disconnectPiProvider.mockResolvedValue(undefined);
    api.deleteProvider.mockResolvedValue(undefined);
    const current = ticket();
    const old = ticket({ id: 'provider:pi-old', sourceId: 'pi-old' });

    await deleteConnectionTicket(current, { isCurrent: true }, actionApi());
    await deleteConnectionTicket(old, { isCurrent: false }, actionApi());

    expect(api.disconnectPiProvider).toHaveBeenCalledWith('pi-current', true);
    expect(api.deleteProvider).toHaveBeenCalledWith('pi', 'pi-old');
    expect(api.disconnectPiProvider).toHaveBeenCalledTimes(1);
  });

  it('keeps the existing undo path for another Agent and allows retry after a failure', async () => {
    vi.resetAllMocks();
    api.undoSwitch.mockResolvedValue(true);
    const other = ticket({
      id: 'provider:workbuddy',
      sourceId: 'workbuddy',
      agentId: 'workbuddy',
    });
    await expect(removeCatalogTicket(other, { isCurrent: true }, actionApi())).resolves.toBe(true);
    expect(api.undoSwitch).toHaveBeenCalledWith('workbuddy');
    expect(api.disconnectPiProvider).not.toHaveBeenCalled();

    const current = ticket({ id: 'provider:pi-failing', sourceId: 'pi-failing' });
    api.disconnectPiProvider.mockRejectedValueOnce(new Error('temporary failure'));
    await expect(removeCatalogTicket(current, { isCurrent: true }, actionApi())).rejects.toThrow('temporary failure');
    api.disconnectPiProvider.mockResolvedValue(undefined);
    await expect(removeCatalogTicket(current, { isCurrent: true }, actionApi())).resolves.toBe(true);
    expect(api.disconnectPiProvider).toHaveBeenCalledTimes(2);
  });

  it('refreshes the wallet after a successful Pi cancellation and suppresses a concurrent second removal', async () => {
    vi.resetAllMocks();
    let resolveDisconnect!: () => void;
    api.disconnectPiProvider.mockImplementationOnce(
      () => new Promise<void>((resolve) => { resolveDisconnect = resolve; }),
    );
    const current = ticket();
    const other = ticket({ id: 'provider:pi-other', sourceId: 'pi-other' });
    const loadWallet = vi.fn(async () => true);
    const poolReload = vi.fn(async () => undefined);
    const actions = mountActions({
      filterAgent: 'all',
      wallet: null,
      extrasForTicket: (row) => row.id === current.id
        ? { isCurrent: true }
        : { isCurrent: true, inList: true },
      loadWallet,
      poolReload,
    });
    const first = actions.handleRemoveFromCatalog(current);
    const second = actions.handleRemoveFromCatalog(other);
    resolveDisconnect();
    await first;
    await second;

    expect(api.disconnectPiProvider).toHaveBeenCalledTimes(1);
    expect(poolReload).toHaveBeenCalledTimes(1);
    expect(loadWallet).toHaveBeenCalledTimes(1);
  });

  it('clears the pending removal after failure so the same current Pi row can retry', async () => {
    vi.resetAllMocks();
    const current = ticket({ id: 'provider:pi-retry', sourceId: 'pi-retry' });
    const loadWallet = vi.fn(async () => true);
    const poolReload = vi.fn(async () => undefined);
    api.disconnectPiProvider.mockRejectedValueOnce(new Error('temporary failure'));
    const actions = mountActions({
      filterAgent: 'all',
      wallet: null,
      extrasForTicket: () => ({ isCurrent: true }),
      loadWallet,
      poolReload,
    });

    await actions.handleRemoveFromCatalog(current);
    api.disconnectPiProvider.mockResolvedValue(undefined);
    await actions.handleRemoveFromCatalog(current);

    expect(api.disconnectPiProvider).toHaveBeenCalledTimes(2);
    expect(poolReload).toHaveBeenCalledTimes(1);
    expect(loadWallet).toHaveBeenCalledTimes(1);
  });
});

describe('Pi live-delete errors', () => {
  it('keeps a Chinese backend explanation and hides English internal summaries', () => {
    expect(describePiLiveDeleteError(
      new Error('没能把这个登录从 Pi 本机正在用的配置里移除，所以没有删除它：config.write'),
      tZh,
    )).toContain('所以没有删除它');
    expect(describePiLiveDeleteError(
      new Error('no auth.json [account.delete.live]'),
      tZh,
    )).toBe('没能从本机配置里移除这份登录，所以没有删除它。');
    expect(describePiLiveDeleteError(
      new Error('auth.json present but credentials could not be classified'),
      tEn,
    )).toBe("Couldn't remove this login from local config, so it was not deleted.");
  });
});

describe('Pi provider action errors', () => {
  it('maps live-config conflict codes to a localized refresh instruction', () => {
    expect(describePiProviderActionError(
      new Error('provider config changed [provider.pi.live_conflict]'),
      tZh,
    )).toBe('Pi 的本机配置已改变，请刷新连接页面后重试。');
    expect(describePiProviderActionError(
      'provider conflict [provider.conflict]',
      tEn,
    )).toBe("Pi's local config changed. Refresh the Connections page and try again.");
  });

  it('uses the generic Pi failure fallback for errors without a stable conflict code', () => {
    expect(describePiProviderActionError(new Error('secret-bearing core detail'), tZh))
      .toBe('无法取消接入，请重试');
    expect(describePiProviderActionError(new Error('secret-bearing core detail'), tEn))
      .toBe("Couldn't disconnect. Try again.");
  });
});

describe('guiErrorCode', () => {
  it('reads a trailing bracket code', async () => {
    const { guiErrorCode } = await import('@/lib/api/settings');
    expect(guiErrorCode('provider switch failed [provider.switch.rollback]')).toBe('provider.switch.rollback');
    expect(guiErrorCode(new Error('io failed [io]'))).toBe('io');
    expect(guiErrorCode('plain text')).toBeUndefined();
  });
});
