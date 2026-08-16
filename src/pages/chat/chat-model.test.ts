import { describe, expect, it } from 'vitest';
import { agentDisplayName } from '@/config/agents';
import type { AgentId, AgentStatus, ChatMessage, Conversation } from '@/lib/types';
import type { AgentProcessView } from '@/lib/chat-process';
import type { TurnGroup } from './chat-format';
import {
  agentHasConfiguredAuth,
  agentPickerLabel,
  blockerCopy,
  blockerPrimaryTarget,
  chatAgentPickerRows,
  chatConnectionKind,
  chatConnectionPickerView,
  connectionPickerCaption,
  conversationTitle,
  cwdShortName,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  messageStatusLabel,
  newConversationDefaults,
  nextConversationAgentIds,
  retryTarget,
  sendBlockers,
  turnComparisonChips,
  visibleAgentDots,
} from './chat-model';

function conv(partial: Partial<Conversation> & Pick<Conversation, 'id'>): Conversation {
  return {
    title: '新对话',
    agentIds: ['claude'],
    cwd: null,
    allowDangerous: false,
    createdAt: '2026-08-16T00:00:00.000Z',
    updatedAt: '2026-08-16T00:00:00.000Z',
    ...partial,
  };
}

function status(
  agentId: AgentId,
  installed: boolean,
  hidden = false,
  extra: Partial<AgentStatus> = {},
): AgentStatus {
  return {
    agentId,
    installed,
    authStatus: installed ? 'valid' : 'none',
    authLabel: installed ? 'API' : '',
    effectiveKind: installed ? 'api' : 'none',
    running: false,
    hidden,
    ...extra,
  };
}

function msg(
  partial: Partial<ChatMessage> & Pick<ChatMessage, 'id' | 'role'>,
): ChatMessage {
  return {
    conversationId: 'c1',
    turn: 1,
    content: '',
    status: 'ok',
    durationMs: 0,
    createdAt: '2026-08-16T00:00:00.000Z',
    ...partial,
  };
}

function processView(phase: AgentProcessView['phase']): AgentProcessView {
  return {
    turn: 1,
    agent: 'claude',
    phase,
    stdout: '',
    stderr: '',
    steps: [],
    updatedAt: 0,
  };
}

describe('cwdShortName', () => {
  it('takes the last segment of a Windows path', () => {
    expect(cwdShortName('D:\\projects\\demo')).toBe('demo');
  });

  it('takes the last segment of a POSIX path', () => {
    expect(cwdShortName('/home/user/proj')).toBe('proj');
  });

  it('returns 未设目录 for null / undefined / empty', () => {
    expect(cwdShortName(null)).toBe('未设目录');
    expect(cwdShortName(undefined)).toBe('未设目录');
    expect(cwdShortName('')).toBe('未设目录');
    expect(cwdShortName('   ')).toBe('未设目录');
  });

  it('strips trailing separators on both styles', () => {
    expect(cwdShortName('D:\\projects\\demo\\')).toBe('demo');
    expect(cwdShortName('/home/user/proj/')).toBe('proj');
    expect(cwdShortName('C:\\\\')).toBe('C:');
    expect(cwdShortName('C:')).toBe('C:');
  });

  it('keeps POSIX root as /', () => {
    expect(cwdShortName('/')).toBe('/');
    expect(cwdShortName('///')).toBe('/');
  });
});

describe('filterConversations', () => {
  const rows = [
    conv({ id: '1', title: 'Fix Login Timeout', cwd: 'D:\\projects\\Demo' }),
    conv({ id: '2', title: '用量页', cwd: '/tmp/usage' }),
  ];

  it('matches title case-insensitively', () => {
    expect(filterConversations(rows, 'login').map((c) => c.id)).toEqual(['1']);
  });

  it('matches cwd case-insensitively', () => {
    expect(filterConversations(rows, 'USAGE').map((c) => c.id)).toEqual(['2']);
  });

  it('returns the original array for an empty / whitespace query', () => {
    expect(filterConversations(rows, '')).toBe(rows);
    expect(filterConversations(rows, '   ')).toBe(rows);
  });
});

describe('groupConversationsByDay', () => {
  // 本地时区 2026-08-16 15:00
  const now = new Date(2026, 7, 16, 15, 0, 0, 0).getTime();

  function at(y: number, m: number, d: number, h = 12): string {
    return new Date(y, m, d, h, 0, 0, 0).toISOString();
  }

  it('buckets across local-day boundaries and drops empty groups', () => {
    const today = conv({ id: 't', title: 'today', updatedAt: at(2026, 7, 16, 1) });
    const yesterday = conv({ id: 'y', title: 'yest', updatedAt: at(2026, 7, 15, 23) });
    const week = conv({ id: 'w', title: 'week', updatedAt: at(2026, 7, 11, 8) });
    const earlier = conv({ id: 'e', title: 'old', updatedAt: at(2026, 7, 9, 8) });
    const groups = groupConversationsByDay([today, yesterday, week, earlier], now);

    expect(groups.map((g) => g.key)).toEqual(['today', 'yesterday', 'week', 'earlier']);
    expect(groups.map((g) => g.label)).toEqual(['今天', '昨天', '近 7 天', '更早']);
    expect(groups.map((g) => g.items.map((c) => c.id))).toEqual([['t'], ['y'], ['w'], ['e']]);
  });

  it('keeps input order inside a group and omits empty buckets', () => {
    const a = conv({ id: 'a', updatedAt: at(2026, 7, 16, 14) });
    const b = conv({ id: 'b', updatedAt: at(2026, 7, 16, 10) });
    const groups = groupConversationsByDay([a, b], now);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe('today');
    expect(groups[0].items.map((c) => c.id)).toEqual(['a', 'b']);
  });

  it('puts today-minus-6 in week and today-minus-7 in earlier', () => {
    const sixDays = conv({ id: 's', updatedAt: at(2026, 7, 10, 12) });
    const sevenDays = conv({ id: 'v', updatedAt: at(2026, 7, 9, 12) });
    const groups = groupConversationsByDay([sixDays, sevenDays], now);
    expect(groups.find((g) => g.key === 'week')?.items.map((c) => c.id)).toEqual(['s']);
    expect(groups.find((g) => g.key === 'earlier')?.items.map((c) => c.id)).toEqual(['v']);
  });
});

describe('sendBlockers', () => {
  const base = conv({
    id: 'cur',
    agentIds: ['claude', 'codex'],
    cwd: 'D:\\work',
  });

  it('returns hiddenAgents before unconfiguredAuth before noCwd before sendingElsewhere', () => {
    const blockers = sendBlockers({
      conversation: { ...base, cwd: null, agentIds: ['claude', 'kimi', 'grok'] },
      hiddenIds: new Set<AgentId>(['kimi']),
      unconfiguredAuthIds: new Set<AgentId>(['grok']),
      sendingConversationId: 'other',
      sendingTitle: '别的会话',
    });
    expect(blockers.map((b) => b.kind)).toEqual([
      'hiddenAgents',
      'unconfiguredAuth',
      'noCwd',
      'sendingElsewhere',
    ]);
    expect(blockers[0]).toEqual({ kind: 'hiddenAgents', agentIds: ['kimi'] });
    expect(blockers[1]).toEqual({ kind: 'unconfiguredAuth', agentIds: ['grok'] });
  });

  it('does not list a hidden agent again as unconfiguredAuth', () => {
    const blockers = sendBlockers({
      conversation: { ...base, agentIds: ['kimi'] },
      hiddenIds: new Set<AgentId>(['kimi']),
      unconfiguredAuthIds: new Set<AgentId>(['kimi']),
      sendingConversationId: null,
    });
    expect(blockers.map((b) => b.kind)).toEqual(['hiddenAgents']);
  });

  it('does not treat an empty draft as a blocker', () => {
    expect(
      sendBlockers({
        conversation: base,
        hiddenIds: new Set(),
        sendingConversationId: null,
      }),
    ).toEqual([]);
  });

  it('ignores sendingElsewhere when the active conversation is the sender', () => {
    expect(
      sendBlockers({
        conversation: base,
        hiddenIds: new Set(),
        sendingConversationId: 'cur',
      }),
    ).toEqual([]);
  });
});

describe('newConversationDefaults', () => {
  const agents = [
    status('claude', true),
    status('codex', true, true),
    status('kimi', false),
    status('grok', true),
  ];

  it('strips hidden and uninstalled ids, keeps the session agent order', () => {
    const active = conv({
      id: 'a',
      // grok 在 claude 前：与 catalog 序（claude, grok）可区分
      agentIds: ['kimi', 'grok', 'codex', 'claude'],
      cwd: '/tmp/app',
    });
    expect(newConversationDefaults(active, agents)).toEqual({
      agentIds: ['grok', 'claude'],
      cwd: '/tmp/app',
    });
  });

  it('falls back to the first installed and visible agent', () => {
    const active = conv({
      id: 'a',
      agentIds: ['codex', 'kimi'],
      cwd: null,
    });
    expect(newConversationDefaults(active, agents)).toEqual({
      agentIds: ['claude'],
      cwd: null,
    });
  });

  it('uses fallback agents and null cwd when there is no active session', () => {
    expect(newConversationDefaults(null, agents)).toEqual({
      agentIds: ['claude'],
      cwd: null,
    });
  });

  it('drops agents without configured auth and falls back to a selectable one', () => {
    const none = status('pi', true, false, {
      authStatus: 'none',
      authLabel: '未配置',
      effectiveKind: 'none',
    });
    const active = conv({
      id: 'a',
      agentIds: ['pi'],
      cwd: '/tmp/app',
    });
    expect(newConversationDefaults(active, [...agents, none])).toEqual({
      agentIds: ['claude'],
      cwd: '/tmp/app',
    });
  });
});

describe('nextConversationAgentIds', () => {
  it('appends a new id and keeps the current primary first', () => {
    expect(nextConversationAgentIds(['grok', 'claude'], 'codex')).toEqual([
      'grok',
      'claude',
      'codex',
    ]);
  });

  it('removes an id without reordering the rest', () => {
    expect(nextConversationAgentIds(['grok', 'claude', 'codex'], 'claude')).toEqual([
      'grok',
      'codex',
    ]);
  });

  it('refuses to drop the last agent', () => {
    expect(nextConversationAgentIds(['claude'], 'claude')).toBeNull();
  });
});

describe('retryTarget', () => {
  const failedLast: TurnGroup[] = [
    {
      turn: 1,
      user: msg({ id: 'u1', role: 'user', turn: 1, content: 'old' }),
      agents: [msg({ id: 'a1', role: 'agent', turn: 1, agentId: 'claude', status: 'ok' })],
    },
    {
      turn: 2,
      user: msg({ id: 'u2', role: 'user', turn: 2, content: 'please retry' }),
      agents: [
        msg({ id: 'a2', role: 'agent', turn: 2, agentId: 'claude', status: 'failed' }),
      ],
    },
  ];

  it('returns the last-turn user prompt when an agent failed', () => {
    expect(retryTarget(failedLast, false)).toEqual({
      turn: 2,
      prompt: 'please retry',
    });
  });

  it('returns null while sending', () => {
    expect(retryTarget(failedLast, true)).toBeNull();
  });

  it('returns null when only a historical turn failed', () => {
    const historical: TurnGroup[] = [
      {
        turn: 1,
        user: msg({ id: 'u1', role: 'user', turn: 1, content: 'old' }),
        agents: [msg({ id: 'a1', role: 'agent', turn: 1, agentId: 'claude', status: 'failed' })],
      },
      {
        turn: 2,
        user: msg({ id: 'u2', role: 'user', turn: 2, content: 'later' }),
        agents: [msg({ id: 'a2', role: 'agent', turn: 2, agentId: 'claude', status: 'ok' })],
      },
    ];
    expect(retryTarget(historical, false)).toBeNull();
  });

  it('returns null without a user prompt', () => {
    const noUser: TurnGroup[] = [
      {
        turn: 1,
        agents: [msg({ id: 'a1', role: 'agent', agentId: 'claude', status: 'cancelled' })],
      },
    ];
    expect(retryTarget(noUser, false)).toBeNull();
  });
});

describe('visibleAgentDots', () => {
  it('shows up to 3 ids and reports the remainder', () => {
    expect(visibleAgentDots(['claude', 'codex', 'kimi'])).toEqual({
      shown: ['claude', 'codex', 'kimi'],
      extra: 0,
    });
    expect(visibleAgentDots(['claude', 'codex', 'kimi', 'grok', 'pi'])).toEqual({
      shown: ['claude', 'codex', 'kimi'],
      extra: 2,
    });
  });
});

describe('conversationTitle', () => {
  it('falls back to 新对话 for empty titles', () => {
    expect(conversationTitle('')).toBe('新对话');
    expect(conversationTitle('   ')).toBe('新对话');
    expect(conversationTitle('修复登录')).toBe('修复登录');
  });
});

describe('blockerCopy', () => {
  it('returns copy for each blocker kind', () => {
    expect(blockerCopy({ kind: 'hiddenAgents', agentIds: ['claude'] })).toEqual({
      text: '会话包含已隐藏 Agent，暂不能发送',
      primaryAction: '去 Agents 页',
    });
    expect(blockerCopy({ kind: 'unconfiguredAuth', agentIds: ['grok'] })).toEqual({
      text: '会话包含未配置授权的 Agent，暂不能发送',
      primaryAction: '去 Connections 页',
    });
    expect(blockerCopy({ kind: 'noCwd' })).toEqual({
      text: '未设置工作目录 — Agent 需要在指定目录内工作',
      primaryAction: '设置工作目录',
    });
    expect(
      blockerCopy({ kind: 'sendingElsewhere', conversationId: 'x', title: '对比方案' }),
    ).toEqual({
      text: '「对比方案」正在生成',
      primaryAction: '回到该会话',
      secondaryAction: '停止',
    });
  });
});

describe('blockerPrimaryTarget', () => {
  it('sends noCwd to the directory picker, not session settings', () => {
    expect(blockerPrimaryTarget({ kind: 'hiddenAgents' })).toBe('agents');
    expect(blockerPrimaryTarget({ kind: 'unconfiguredAuth' })).toBe('connections');
    expect(blockerPrimaryTarget({ kind: 'noCwd' })).toBe('pick-directory');
    expect(blockerPrimaryTarget({ kind: 'sendingElsewhere' })).toBe('settings');
  });
});

describe('connectionPickerCaption', () => {
  it('returns the primary-agent caption only for multi-select', () => {
    expect(connectionPickerCaption({ agentIds: ['claude'] })).toBeNull();
    expect(
      connectionPickerCaption({ agentIds: ['claude', 'codex'], primaryAgent: 'claude' }),
    ).toBe(`仅作用于首位 Agent（${agentDisplayName('claude')}）`);
  });
});

describe('agentPickerLabel', () => {
  it('labels none / single / multi sessions', () => {
    expect(agentPickerLabel(null)).toBe('选择 Agent');
    expect(agentPickerLabel(conv({ id: '1', agentIds: ['claude'] }))).toBe(
      agentDisplayName('claude'),
    );
    expect(agentPickerLabel(conv({ id: '2', agentIds: ['claude', 'codex'] }))).toBe(
      '2 个 Agent',
    );
  });
});

describe('messageStatusLabel', () => {
  it('returns null for success statuses', () => {
    expect(messageStatusLabel('ok')).toBeNull();
    expect(messageStatusLabel('done')).toBeNull();
    expect(messageStatusLabel('success')).toBeNull();
  });

  it('uses process phase while running', () => {
    expect(messageStatusLabel('running', processView('queued'))).toBe('排队中');
    expect(messageStatusLabel('running', processView('starting'))).toBe('启动中');
    expect(messageStatusLabel('running', processView('running'))).toBe('生成中');
    expect(messageStatusLabel('running')).toBe('生成中');
  });

  it('maps terminal and unknown statuses', () => {
    expect(messageStatusLabel('failed')).toBe('失败');
    expect(messageStatusLabel('cancelled')).toBe('已取消');
    expect(messageStatusLabel('timeout')).toBe('超时');
    expect(messageStatusLabel('weird')).toBe('weird');
  });
});

describe('turnComparisonChips', () => {
  it('maps agent messages to chip rows', () => {
    const chips = turnComparisonChips([
      msg({
        id: 'm1',
        role: 'agent',
        agentId: 'claude',
        status: 'ok',
        durationMs: 1200,
      }),
      msg({
        id: 'm2',
        role: 'agent',
        agentId: 'codex',
        status: 'running',
        durationMs: 0,
      }),
    ]);
    expect(chips).toEqual([
      { agentId: 'claude', status: 'ok', durationMs: 1200, messageId: 'm1' },
      { agentId: 'codex', status: 'running', durationMs: 0, messageId: 'm2' },
    ]);
  });
});

describe('agentHasConfiguredAuth / picker rows', () => {
  it('treats bound account/api and verified health as configured', () => {
    expect(agentHasConfiguredAuth(status('claude', true))).toBe(true);
    expect(
      agentHasConfiguredAuth(
        status('codex', true, false, {
          effectiveKind: 'account',
          authHealth: 'verified',
          authStatus: 'valid',
        }),
      ),
    ).toBe(true);
    expect(
      agentHasConfiguredAuth(
        status('kimi', true, false, {
          effectiveKind: 'none',
          authStatus: 'none',
          authLabel: '未配置',
          authHealth: 'missing',
        }),
      ),
    ).toBe(false);
    expect(agentHasConfiguredAuth(status('grok', false))).toBe(false);
  });

  it('isChatAgentSelectable requires installed, visible, and configured auth', () => {
    expect(isChatAgentSelectable(status('claude', true))).toBe(true);
    expect(isChatAgentSelectable(status('codex', true, true))).toBe(false);
    expect(
      isChatAgentSelectable(
        status('kimi', true, false, {
          effectiveKind: 'none',
          authStatus: 'none',
          authHealth: 'missing',
        }),
      ),
    ).toBe(false);
  });

  it('lists selectable agents first and parks hidden / no-auth at the end', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['claude', 'codex', 'kimi', 'grok', 'pi'],
      selectedIds: ['claude'],
      agentStatus: [
        status('claude', true),
        status('codex', true, true),
        status('kimi', true, false, {
          effectiveKind: 'none',
          authStatus: 'none',
          authHealth: 'missing',
        }),
        status('grok', false),
        status('pi', true),
      ],
    });
    expect(rows.map((r) => r.id)).toEqual(['claude', 'pi', 'codex', 'kimi']);
    expect(rows.map((r) => r.selectable)).toEqual([true, true, false, false]);
    expect(rows.map((r) => r.reason)).toEqual([null, null, 'hidden', 'noAuth']);
  });

  it('keeps an already-selected uninstalled agent visible but unselectable', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['claude', 'kimi'],
      selectedIds: ['kimi'],
      agentStatus: [status('claude', true), status('kimi', false)],
    });
    expect(rows.map((r) => r.id)).toEqual(['claude', 'kimi']);
    expect(rows[1]).toEqual({ id: 'kimi', selectable: false, reason: 'noAuth' });
  });
});

describe('chatConnectionPickerView', () => {
  it('does not treat a current oauth account as unconfigured when no API provider exists', () => {
    const grok = status('grok', true, false, {
      effectiveKind: 'account',
      effectiveLabel: 'user@example.com',
      authHealth: 'renewable',
      authLabel: '可续期·未验证',
    });
    expect(chatConnectionKind(grok, false)).toBe('account');
    const view = chatConnectionPickerView({
      primaryAgent: 'grok',
      status: grok,
    });
    expect(view.kind).toBe('account');
    expect(view.label).toBe('user@example.com');
    expect(view.subtitle).toBeNull();
    expect(view.currentLoginTitle).toBe('user@example.com');
    expect(view.currentLoginSubtitle).toBe('当前登录');
    expect(view.emptyHint).toBeNull();
    expect(view.manageLabel).toBe('去 Connections 管理');
  });

  it('keeps API provider name and model when that is the effective connection', () => {
    const view = chatConnectionPickerView({
      primaryAgent: 'claude',
      status: status('claude', true, false, {
        effectiveKind: 'api',
        effectiveLabel: 'api.example.com',
      }),
      currentProviderName: 'api.example.com',
      currentProviderModel: 'sonnet',
    });
    expect(view.kind).toBe('api');
    expect(view.label).toBe('api.example.com');
    expect(view.subtitle).toBe('sonnet');
    expect(view.currentLoginTitle).toBeNull();
    expect(view.manageLabel).toBe('去 Connections 管理');
  });

  it('prefers the bound account over a leftover provider row', () => {
    const view = chatConnectionPickerView({
      primaryAgent: 'grok',
      status: status('grok', true, false, {
        effectiveKind: 'account',
        effectiveLabel: 'user@example.com',
      }),
      currentProviderName: 'stale-api',
      currentProviderModel: 'grok-4',
    });
    expect(view.kind).toBe('account');
    expect(view.label).toBe('user@example.com');
    expect(view.subtitle).toBeNull();
  });

  it('treats live oauth without a wallet current row as logged in, not missing', () => {
    const grok = status('grok', true, false, {
      effectiveKind: 'none',
      effectiveLabel: undefined,
      authHealth: 'renewable',
      authStatus: 'valid',
      authLabel: '可续期·未验证',
    });
    expect(chatConnectionKind(grok, true)).toBe('account');
    const view = chatConnectionPickerView({
      primaryAgent: 'grok',
      status: grok,
      currentProviderName: 'stale-api',
    });
    expect(view.kind).toBe('account');
    expect(view.label).toBe('已登录');
    expect(view.emptyHint).toBeNull();
  });

  it('treats live API credentials without a current provider as API, not a login row', () => {
    const view = chatConnectionPickerView({
      primaryAgent: 'claude',
      status: status('claude', true, false, {
        effectiveKind: 'none',
        effectiveLabel: undefined,
        authHealth: 'configured',
        authStatus: 'valid',
        authLabel: '已配置·未验证',
      }),
    });
    expect(view.kind).toBe('api');
    expect(view.label).toBe('API');
    expect(view.currentLoginSubtitle).toBe('API');
    expect(view.emptyHint).toBeNull();
  });

  it('keeps 未配置连接 only when the agent has no bound login or API', () => {
    const view = chatConnectionPickerView({
      primaryAgent: 'pi',
      status: status('pi', true, false, {
        effectiveKind: 'none',
        authStatus: 'none',
        authLabel: '未配置',
        authHealth: 'missing',
      }),
    });
    expect(view.kind).toBe('none');
    expect(view.label).toBe('未配置连接');
    expect(view.emptyHint).toBe('暂无连接');
    expect(view.manageLabel).toBe('去 Connections 添加');
  });

  it('replaces the chip label while switching', () => {
    const view = chatConnectionPickerView({
      primaryAgent: 'claude',
      switching: true,
      status: status('claude', true),
      currentProviderName: 'official',
    });
    expect(view.label).toBe('切换中…');
    expect(view.subtitle).toBeNull();
  });
});
