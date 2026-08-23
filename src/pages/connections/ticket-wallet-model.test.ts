import { describe, expect, it, vi } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { agentDisplayName } from '@/config/agents';
import type { Account, Provider } from '@/lib/types';
import type { TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  focusedTicketAddAgent,
  dispatchTicketAddAction,
  armMenuDialogOpen,
  handleMenuDialogSelect,
  handleTicketAddMenuSelect,
  MENU_DIALOG_DISMISS_CLEAR_MS,
  shouldIgnoreMenuDialogDismiss,
  ticketAddDialogState,
  buildTicketDetailFields,
  buildTicketWalletRows,
  countTicketsByFilter,
  dashboardBindingMetaText,
  extrasFromPoolSource,
  filterTickets,
  hasOfficialQuotaWindow,
  findTicketPoolSource,
  formatTicketBindingDetailLines,
  formatTicketUsageParts,
  formatTicketUsageText,
  humanizeTicketAuthLabel,
  ticketAuthChip,
  ticketCardTitle,
  ticketSwitchChip,
  showsNativeSwitch,
  isUnrecognizedTicket,
  ticketBindingStatus,
  ticketDetailEditLabel,
  ticketWalletFilterLabel,
  ticketCredentialClassChipLabel,
} from './ticket-wallet-model';

function sampleWallet(): TicketWallet {
  return {
    tickets: [
      {
        id: 'provider:kimi-1',
        sourceKind: 'provider',
        sourceId: 'kimi-1',
        agentId: 'kimi',
        label: 'Kimi 会员',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: ['anthropic-messages', 'openai-chat'],
        importedFrom: 'kimi',
      },
      {
        id: 'provider:ant-1',
        sourceKind: 'provider',
        sourceId: 'ant-1',
        agentId: 'claude',
        label: 'Anthropic Key',
        surface: 'anthropic-api',
        credentialClass: 'api_key',
        speaks: ['anthropic-messages'],
        importedFrom: 'claude',
      },
      {
        id: 'provider:unk-1',
        sourceKind: 'provider',
        sourceId: 'unk-1',
        agentId: 'claude',
        label: '自定义中转',
        // Production shape: unknown surface keeps real credential class.
        surface: 'unknown',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'claude',
      },
      {
        id: 'account:oauth-1',
        sourceKind: 'account',
        sourceId: 'oauth-1',
        agentId: 'claude',
        label: 'me@example.com',
        surface: 'unknown',
        credentialClass: 'oauth',
        speaks: [],
        importedFrom: 'claude',
      },
    ],
    bindings: [
      {
        ticketId: 'provider:kimi-1',
        agentId: 'claude',
        route: 'reshape',
        active: true,
        profileId: 'p1',
        bridge: null,
      },
      {
        ticketId: 'provider:kimi-1',
        agentId: 'codex',
        route: 'bridge',
        active: true,
        profileId: 'p2',
        bridge: { port: 8123, running: true },
      },
      {
        ticketId: 'account:oauth-1',
        agentId: 'claude',
        route: 'native',
        active: false,
        profileId: null,
        bridge: null,
      },
    ],
    surfaceGroups: [],
  };
}

describe('ticket wallet filter', () => {
  it('counts and filters 未识别 by surface (production unknown + api_key shape)', () => {
    const tickets = sampleWallet().tickets;
    expect(isUnrecognizedTicket(tickets[2]!)).toBe(true);
    expect(countTicketsByFilter(tickets)).toEqual({
      all: 4,
      oauth: 1,
      api_key: 3,
      unknown: 2,
    });
    expect(filterTickets(tickets, 'oauth').map((t) => t.id)).toEqual(['account:oauth-1']);
    expect(filterTickets(tickets, 'unknown').map((t) => t.id)).toEqual([
      'provider:unk-1',
      'account:oauth-1',
    ]);
  });
});

describe('hasOfficialQuotaWindow', () => {
  it('hides missing official percents and shows 0 as a real value', () => {
    expect(hasOfficialQuotaWindow(undefined)).toBe(false);
    expect(hasOfficialQuotaWindow(null)).toBe(false);
    expect(hasOfficialQuotaWindow(Number.NaN)).toBe(false);
    expect(hasOfficialQuotaWindow(0)).toBe(true);
    expect(hasOfficialQuotaWindow(40)).toBe(true);
  });
});

describe('binding usage text', () => {
  it('formats active bindings with route labels', () => {
    const wallet = sampleWallet();
    const kimiBindings = wallet.bindings.filter((b) => b.ticketId === 'provider:kimi-1');
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('正用于：');
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('改配置');
    expect(formatTicketUsageText(kimiBindings, 'kimi')).toContain('本机路由 · 运行中');
    expect(formatTicketUsageText([])).toBe('未使用');
    expect(formatTicketUsageText([], 'codex')).toBe(`${agentDisplayName('codex')} · 未使用`);
    const parts = formatTicketUsageParts(kimiBindings, 'kimi');
    expect(parts.some((part) => part.kind === 'bridge' && part.href === '/routes?profile=p2')).toBe(true);
    expect(formatTicketUsageParts([{
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: null,
      bridge: { port: 8123, running: true },
    }]).some((part) => part.kind === 'bridge' && part.href === '/routes')).toBe(true);
  });

  it('annotates bridge usage with N-member poll pool copy', () => {
    const wallet = sampleWallet();
    wallet.surfaceGroups = [{
      surface: 'kimi-code-membership',
      credentialClass: 'api_key',
      members: [
        {
          ticketId: 'account:kimi-stale',
          sourceKind: 'account',
          sourceId: 'kimi-stale',
          agentId: 'kimi',
          label: 'Kimi 会员（失效号）',
          health: 'needs_login',
        },
        {
          ticketId: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'Kimi 会员',
          health: 'renewable',
        },
      ],
    }];
    const rows = buildTicketWalletRows(wallet);
    const kimi = rows.find((row) => row.ticket.id === 'provider:kimi-1');
    expect(kimi?.usageText).toContain('2 份同类登录可轮换');
    expect(kimi?.usageText).toContain('本机路由');
    expect(kimi?.usageText).toContain('运行中');
    const ant = rows.find((row) => row.ticket.id === 'provider:ant-1');
    expect(ant?.usageText).not.toContain('可轮换');
  });

  it('keeps self-use on one phrase so the row does not repeat the owner', () => {
    expect(formatTicketUsageText([{
      ticketId: 'account:codex-1',
      agentId: 'codex',
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    }], 'codex')).toBe(`${agentDisplayName('codex')}（切换）`);
    expect(formatTicketUsageText([{
      ticketId: 'account:codex-1',
      agentId: 'codex',
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    }], 'codex')).not.toContain('正用于：');
  });

  it('maps dashboard meta text', () => {
    expect(dashboardBindingMetaText('Kimi 会员', 'reshape')).toBe('Kimi 会员 · 改配置');
    expect(dashboardBindingMetaText('Kimi 会员', 'bridge')).toBe('Kimi 会员 · 本机路由');
    expect(dashboardBindingMetaText('me@…', 'native')).toBe('me@… · 直连');
  });
});

describe('buildTicketWalletRows', () => {
  it('highlights deep-link agent active bindings without privatizing the list', () => {
    const wallet = sampleWallet();
    const rows = buildTicketWalletRows(wallet, { highlightAgentId: 'claude' });
    expect(rows).toHaveLength(4);
    const kimi = rows.find((r) => r.ticket.id === 'provider:kimi-1');
    const oauth = rows.find((r) => r.ticket.id === 'account:oauth-1');
    expect(kimi?.highlighted).toBe(true);
    expect(oauth?.highlighted).toBe(false);
  });

  it('finds active binding for dashboard agent', () => {
    const wallet = sampleWallet();
    const hit = activeBindingForAgent(wallet, 'codex');
    expect(hit?.ticket.label).toBe('Kimi 会员');
    expect(hit?.binding.route).toBe('bridge');
    expect(activeBindingForAgent(wallet, 'pi')).toBeNull();
  });
});

function ticket(partial: Partial<TicketView> & Pick<TicketView, 'id'>): TicketView {
  return {
    sourceKind: 'provider',
    sourceId: 'kimi-1',
    agentId: 'kimi',
    label: 'Kimi 会员',
    surface: 'kimi-code-membership',
    credentialClass: 'api_key',
    speaks: ['anthropic-messages'],
    importedFrom: 'kimi',
    ...partial,
  };
}

function account(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'claude',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function provider(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'claude',
    preset: 'custom',
    configText: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com',
        ANTHROPIC_AUTH_TOKEN: '***',
      },
    }),
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

describe('ticket detail fields', () => {
  it('keeps API Key custom endpoint facts under advanced only', () => {
    const { advanced } = buildTicketDetailFields(ticket({ id: 'provider:kimi-1' }), {
      endpointMode: 'custom',
      endpointHost: 'https://relay.example.com/v1',
    });
    expect(advanced).toEqual(expect.arrayContaining([
      { label: '端点', value: '自定义' },
      { label: '主机', value: 'relay.example.com', mono: true },
      { label: '协议', value: 'anthropic-messages' },
    ]));
    const customLabels = advanced.map((field) => field.label);
    expect(customLabels).not.toContain('导入自');
    expect(customLabels).not.toContain('登录状态');
    expect(customLabels).not.toEqual(
      expect.arrayContaining(['类型', '来源', '所属', '官方账号', '提供商']),
    );
  });

  it('omits import, login status, and protocol for official OAuth', () => {
    const { advanced } = buildTicketDetailFields(
      ticket({
        id: 'account:oauth-1',
        sourceKind: 'account',
        sourceId: 'oauth-1',
        agentId: 'grok',
        label: 'me@example.com',
        surface: 'grok-xai-subscription',
        credentialClass: 'oauth',
        speaks: ['openai-chat', 'xai-device-code'],
        importedFrom: 'grok',
      }),
      {
        identity: 'me@example.com',
        accountProvider: 'https://accounts.x.ai/oauth',
        authLabel: '可续期·未验证',
        endpointMode: 'official',
        endpointHost: 'accounts.x.ai',
      },
    );
    const labels = advanced.map((field) => field.label);
    expect(advanced).toEqual([]);
    expect(labels).not.toContain('导入自');
    expect(labels).not.toContain('登录状态');
    expect(labels).not.toContain('类型');
    expect(labels).not.toContain('来源');
    expect(labels).not.toContain('所属');
    expect(labels).not.toContain('官方账号');
    expect(labels).not.toContain('协议');
    expect(labels).not.toContain('提供商');
    expect(labels).not.toContain('端点');
    expect(labels).not.toContain('Endpoint');
  });

  it('humanizes login health without 未验证', () => {
    expect(humanizeTicketAuthLabel('可续期·未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置·未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期，尚未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置，尚未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置')).toBe('已配置');
    expect(humanizeTicketAuthLabel('已验证')).toBe('已验证');
  });

  it('replaces 可续期 / 已配置 chips with the secret tail', () => {
    expect(ticketAuthChip({
      authLabel: '可续期·未验证',
      secretTail: '**JF6Q',
    })).toEqual({ label: '**JF6Q', mono: true });
    expect(ticketAuthChip({
      authLabel: '已配置',
      secretTail: '**wxyz',
    })).toEqual({ label: '**wxyz', mono: true });
    expect(ticketAuthChip({ authLabel: '可续期·未验证' })).toEqual({
      label: '可续期',
      mono: false,
    });
    expect(ticketAuthChip({ authLabel: '已验证', secretTail: '**JF6Q' })).toEqual({
      label: '已验证',
      mono: false,
    });
  });

  it('prefers healed email over placeholder ticket labels', () => {
    expect(ticketCardTitle(
      { label: 'codex oauth' },
      { identity: 'user@example.com' },
    )).toBe('user@example.com');
    expect(ticketCardTitle(
      { label: 'codex-oauth' },
      { accountLabel: 'user@example.com' },
    )).toBe('user@example.com');
    expect(ticketCardTitle(
      { label: 'codex oauth' },
      { identity: '官方未提供账号信息', accountLabel: 'codex-oauth' },
    )).toBe('codex oauth');
  });

  it('hides native 切换 on a foreign Agent usage tab', () => {
    expect(showsNativeSwitch('kimi', null)).toBe(true);
    expect(showsNativeSwitch('kimi', 'kimi')).toBe(true);
    expect(showsNativeSwitch('kimi', 'codex')).toBe(false);
  });

  it('uses 切换 for idle grants and 使用中 when current', () => {
    expect(ticketSwitchChip()).toEqual({ kind: 'switch', label: '切换' });
    expect(ticketSwitchChip({ isCurrent: false })).toEqual({ kind: 'switch', label: '切换' });
    expect(ticketSwitchChip({ isCurrent: true })).toEqual({ kind: 'in-use', label: '使用中' });
  });

  it('lists bindings as agent + one short status', () => {
    const wallet = sampleWallet();
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'provider:kimi-1'),
    )).toEqual([
      { agent: agentDisplayName('claude'), status: '当前使用' },
      { agent: 'http://127.0.0.1:8123/v1/responses', status: '本机路由运行中' },
    ]);
    expect(formatTicketBindingDetailLines(
      wallet.bindings.filter((binding) => binding.ticketId === 'account:oauth-1'),
    )).toEqual([{ agent: agentDisplayName('claude'), status: '未使用' }]);
    expect(ticketBindingStatus({
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'p2',
      bridge: { port: 8123, running: false },
    })).toBe('本机路由已停止');
    expect(humanizeTicketAuthLabel('可续期·未验证')).toBe('可续期');
    expect(humanizeTicketAuthLabel('已配置·未验证')).toBe('已配置');
    expect(humanizeTicketAuthLabel('可续期')).toBe('可续期');
  });

  it('joins pool extras so OAuth can be inspected and API Key can be edited', () => {
    const oauth = ticket({
      id: 'account:oauth-1',
      sourceKind: 'account',
      sourceId: 'oauth-1',
      agentId: 'claude',
      label: 'me@example.com',
      surface: 'claude-subscription',
      credentialClass: 'oauth',
      speaks: [],
      importedFrom: 'claude',
    });
    const source = findTicketPoolSource(oauth, [
      account({
        id: 'oauth-1',
        kind: 'oauth',
        label: 'me@example.com',
        email: 'me@example.com',
        subscription: 'Pro',
        quota5hPct: 40,
      }),
    ], []);
    const extras = extrasFromPoolSource(oauth, source);
    expect(extras.identity).toBe('me@example.com');
    expect(extras.accountLabel).toBe('me@example.com');
    expect(extras.isCurrent).toBe(false);
    expect(extrasFromPoolSource(oauth, source, undefined, 'account:oauth-1').isCurrent).toBe(true);
    expect(extrasFromPoolSource(oauth, source, undefined, 'provider:kimi-1').isCurrent).toBe(false);
    expect(extras.canEditKey).toBe(false);
    expect(extras.canEditConfig).toBe(false);
    expect(extras.oauthAction).toEqual({ kind: 'refresh-quota', label: '刷新' });
    expect(extras.refreshTokenPreview).toBeUndefined();
    expect(ticketDetailEditLabel(extras)).toBeNull();

    const previewExtras = extrasFromPoolSource(oauth, {
      account: account({
        id: 'oauth-1',
        kind: 'oauth',
        label: 'me@example.com',
        email: 'me@example.com',
        refreshTokenPreview: 'rt--••••wxyz',
        secretTail: '**wxyz',
      }),
    });
    expect(previewExtras.refreshTokenPreview).toBe('rt--••••wxyz');
    expect(previewExtras.secretTail).toBe('**wxyz');

    const keyExtrasFromAccount = extrasFromPoolSource(
      ticket({
        id: 'account:key-1',
        sourceKind: 'account',
        sourceId: 'key-1',
        agentId: 'kimi',
        label: 'Kimi key',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
      }),
      {
        account: account({
          id: 'key-1',
          kind: 'apikey',
          label: 'Kimi key',
          secretTail: '**here',
        }),
      },
    );
    expect(keyExtrasFromAccount.secretTail).toBe('**here');

    const keyTicket = ticket({ id: 'provider:kimi-1' });
    const keyExtras = extrasFromPoolSource(
      keyTicket,
      findTicketPoolSource(keyTicket, [], [
        provider({ id: 'kimi-1', agentId: 'kimi', name: 'Kimi 会员', secretTail: '**wxyz' }),
      ]),
    );
    expect(keyExtras.canEditConfig).toBe(true);
    expect(keyExtras.endpointHost).toBe('relay.example.com');
    expect(keyExtras.secretTail).toBe('**wxyz');
    expect(ticketDetailEditLabel(keyExtras)).toBe('编辑配置');
  });

  it('splits Grok oauth extras by Hub vs CLI ownership', () => {
    const grokTicket = ticket({
      id: 'account:grok-1',
      sourceKind: 'account',
      sourceId: 'grok-1',
      agentId: 'grok',
      label: 'user@x.ai',
      surface: 'grok-xai-subscription',
      credentialClass: 'oauth',
      speaks: [],
      importedFrom: 'grok',
    });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'oauth_pkce',
        refreshable: true,
      }),
    }).oauthAction).toEqual({ kind: 'refresh-credentials', label: '刷新' });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'live',
        isCurrent: true,
      }),
    }).oauthAction).toEqual({ kind: 'sync-current-login', label: '同步当前登录' });
    expect(extrasFromPoolSource(grokTicket, {
      account: account({
        id: 'grok-1',
        agentId: 'grok',
        kind: 'oauth',
        label: 'user@x.ai',
        source: 'auth.json',
        isCurrent: false,
      }),
    }).oauthAction).toEqual({ kind: 'refresh-quota', label: '刷新' });
  });
});

describe('buildTicketAddMenu', () => {
  it('nests import and API Key under each Agent', () => {
    const menu = buildTicketAddMenu(['claude', 'kimi']);
    expect(menu.map((item) => item.id)).toEqual(['claude', 'kimi']);
    expect(menu[0]?.name).toBe(agentDisplayName('claude'));
    expect(menu.map((item) => item.actions.map((a) => a.kind))).toEqual([
      ['import-login', 'api-key'],
      ['import-login', 'api-key'],
    ]);
    expect(menu[0]?.actions.map((a) => a.label)).toEqual(['导入当前授权', '添加 API Key']);
  });

  it('is empty when no Agent is installed', () => {
    expect(buildTicketAddMenu([])).toEqual([]);
    expect(buildTicketAddMenu(null)).toEqual([]);
    expect(buildTicketAddMenu()).toEqual([]);
  });

  it('focuses the selected Agent tab so Add skips the picker', () => {
    const menu = buildTicketAddMenu(['claude', 'kimi']);
    expect(focusedTicketAddAgent(menu, null)).toBeNull();
    expect(focusedTicketAddAgent(menu, 'kimi')?.id).toBe('kimi');
    expect(focusedTicketAddAgent(menu, 'grok')).toBeNull();
  });
});

describe('dispatchTicketAddAction', () => {
  it('forwards the selected Agent to the matching handler', () => {
    const onImportLogin = vi.fn();
    const onAddKey = vi.fn();
    dispatchTicketAddAction('import-login', 'kimi', { onImportLogin, onAddKey });
    expect(onImportLogin).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledWith('kimi');
    expect(onAddKey).not.toHaveBeenCalled();

    onImportLogin.mockClear();
    dispatchTicketAddAction('api-key', 'claude', { onImportLogin, onAddKey });
    expect(onAddKey).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledWith('claude');
    expect(onImportLogin).not.toHaveBeenCalled();
  });

  it('no-ops when the matching handler is missing', () => {
    expect(() => dispatchTicketAddAction('import-login', 'kimi', {})).not.toThrow();
    expect(() => dispatchTicketAddAction('api-key', 'claude', {})).not.toThrow();
  });
});

describe('ticketAddDialogState', () => {
  it('opens import or API Key against the submenu Agent', () => {
    expect(ticketAddDialogState('import-login', 'codex')).toEqual({
      addAgentId: 'codex',
      loginImportOpen: true,
      apiKeyDialogOpen: false,
      clearEditProvider: false,
    });
    expect(ticketAddDialogState('api-key', 'grok')).toEqual({
      addAgentId: 'grok',
      loginImportOpen: false,
      apiKeyDialogOpen: true,
      clearEditProvider: true,
    });
  });
});

describe('handleTicketAddMenuSelect', () => {
  it('swallows the menu select, opens the matching dialog, then closes the menu', () => {
    const event = { preventDefault: vi.fn() };
    const onImportLogin = vi.fn();
    const onAddKey = vi.fn();
    const onMenuClose = vi.fn();
    const schedule = vi.fn<(fn: () => void) => void>();

    handleTicketAddMenuSelect(event, 'import-login', 'kimi', {
      onImportLogin,
      onAddKey,
      onMenuClose,
    }, schedule);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledOnce();
    expect(onImportLogin).toHaveBeenCalledWith('kimi');
    expect(onAddKey).not.toHaveBeenCalled();
    expect(onMenuClose).not.toHaveBeenCalled();
    expect(schedule).toHaveBeenCalledOnce();
    schedule.mock.calls[0]![0]();
    expect(onMenuClose).toHaveBeenCalledOnce();
  });

  it('opens the add API Key dialog without touching the wallet filter', () => {
    const event = { preventDefault: vi.fn() };
    const onAddKey = vi.fn();
    handleTicketAddMenuSelect(event, 'api-key', 'claude', { onAddKey });
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledOnce();
    expect(onAddKey).toHaveBeenCalledWith('claude');
    expect(ticketAddDialogState('api-key', 'claude')).toMatchObject({
      apiKeyDialogOpen: true,
      loginImportOpen: false,
    });
  });
});

describe('shouldIgnoreMenuDialogDismiss', () => {
  it('ignores only a close while the opening click is still settling', () => {
    expect(shouldIgnoreMenuDialogDismiss(true, false)).toBe(true);
    expect(shouldIgnoreMenuDialogDismiss(true, true)).toBe(false);
    expect(shouldIgnoreMenuDialogDismiss(false, false)).toBe(false);
  });
});

describe('armMenuDialogOpen', () => {
  it('arms, opens, then clears after the menu-close delay', () => {
    const arm = { current: false };
    const open = vi.fn();
    const schedule = vi.fn<(fn: () => void, delayMs?: number) => void>();

    armMenuDialogOpen(arm, open, MENU_DIALOG_DISMISS_CLEAR_MS, schedule);

    expect(arm.current).toBe(true);
    expect(open).toHaveBeenCalledOnce();
    expect(schedule).toHaveBeenCalledOnce();
    expect(schedule.mock.calls[0]![1]).toBe(100);
    expect(shouldIgnoreMenuDialogDismiss(arm.current, false)).toBe(true);
    schedule.mock.calls[0]![0]();
    expect(arm.current).toBe(false);
    expect(shouldIgnoreMenuDialogDismiss(arm.current, false)).toBe(false);
  });
});

describe('handleMenuDialogSelect', () => {
  it('preventDefault then arms the same openTicketAdd ignore window', () => {
    const event = { preventDefault: vi.fn() };
    const arm = { current: false };
    const open = vi.fn();
    const schedule = vi.fn<(fn: () => void, delayMs?: number) => void>();

    handleMenuDialogSelect(event, arm, open, MENU_DIALOG_DISMISS_CLEAR_MS, schedule);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(open).toHaveBeenCalledOnce();
    expect(arm.current).toBe(true);
    schedule.mock.calls[0]![0]();
    expect(arm.current).toBe(false);
  });
});

describe('filter change after add-dialog leftover', () => {
  it('does not throw when returning to 全部 with mixed or incomplete tickets', () => {
    const wallet: TicketWallet = {
      tickets: [
        {
          id: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'Kimi 会员',
          surface: 'kimi-code-membership',
          credentialClass: 'api_key',
          speaks: ['anthropic-messages'],
          importedFrom: 'kimi',
        },
        {
          id: 'account:codex-1',
          sourceKind: 'account',
          sourceId: 'codex-1',
          agentId: 'codex',
          label: 'ChatGPT Plus',
          surface: 'codex-chatgpt-subscription',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'codex',
        },
        {
          id: 'account:unknown-1',
          sourceKind: 'account',
          sourceId: 'u1',
          agentId: 'pi',
          label: '',
          surface: 'unknown',
          credentialClass: 'unknown',
          speaks: undefined as unknown as string[],
          importedFrom: null,
        },
      ],
      bindings: [
        {
          ticketId: 'account:codex-1',
          agentId: 'codex',
          route: 'native',
          active: true,
          profileId: null,
          bridge: null,
        },
        {
          ticketId: 'provider:kimi-1',
          agentId: 'claude',
          route: 'bridge',
          active: true,
          profileId: null,
          bridge: { port: null, running: false },
        },
      ],
      surfaceGroups: [],
    };

    expect(() => buildTicketWalletRows(wallet, { filter: 'api_key' })).not.toThrow();
    expect(() => buildTicketWalletRows(wallet, { filter: 'all' })).not.toThrow();
    const allRows = buildTicketWalletRows(wallet, { filter: 'all' });
    expect(allRows).toHaveLength(3);
    expect(() =>
      extrasFromPoolSource(wallet.tickets[2]!, { account: undefined, provider: undefined }),
    ).not.toThrow();
    expect(() => buildTicketDetailFields(wallet.tickets[2]!)).not.toThrow();
  });
});

describe('ticket wallet labels with translator', () => {
  it('uses kind / connections copy', () => {
    const t = createTranslator('en');
    expect(ticketWalletFilterLabel('all', t)).toBe('All');
    expect(ticketWalletFilterLabel('oauth', t)).toBe('Official login');
    expect(ticketCredentialClassChipLabel('api_key', t)).toBe('API Key');
    const wallet = sampleWallet();
    const rows = buildTicketWalletRows(wallet, { t });
    const kimi = rows.find((r) => r.ticket.id === 'provider:kimi-1');
    expect(kimi?.usageText).toContain('Rewrite config');
    expect(kimi?.usageText).toContain('Local route');
    expect(kimi?.usageText).not.toContain('改配置');
  });
});
