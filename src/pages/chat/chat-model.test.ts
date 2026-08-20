import { describe, expect, it } from 'vitest';
import { agentDisplayName } from '@/config/agents';
import { createTranslator } from '@/lib/i18n';
import type { Account, AgentId, AgentStatus, ChatMessage, Conversation, Provider } from '@/lib/types';
import type { AgentProcessView } from '@/lib/chat-process';
import type { TurnGroup } from './chat-format';
import {
  agentHasConfiguredAuth,
  agentPickerLabel,
  blockerCopy,
  blockerPrimaryTarget,
  chatAgentPickerEmptyCopy,
  chatAgentPickerEmptyKind,
  chatAgentPickerRows,
  chatConnectionKind,
  chatConnectionOptions,
  leftoverBindTicketId,
  leftoverProviderIsCurrent,
  clampComposerTextareaHeight,
  COMPOSER_TEXTAREA_MAX_PX,
  COMPOSER_TEXTAREA_MIN_PX,
  composerTextareaMeasuredStyle,
  composerTextareaOverflowY,
  composerUsesCssFieldSizing,
  chatTranscriptSurfaceClass,
  chatComposerChromeClass,
  chatConnectionPickerView,
  connectionPickerCaption,
  isLeftoverLocalRouteProvider,
  conversationResumeCommand,
  conversationTitle,
  cwdShortName,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  messageStatusLabel,
  newConversationDefaults,
  autoApproveActive,
  autoApproveConfirmCopy,
  autoApproveEffect,
  autoApproveFooter,
  autoApproveHint,
  selectConversationAgent,
  retryTarget,
  sendBlockers,
  turnComparisonChips,
  visibleAgentDots,
} from './chat-model';

const t = createTranslator('zh');

describe('conversationResumeCommand', () => {
  it('returns the official TUI command when a native session is linked', () => {
    expect(
      conversationResumeCommand({
        agentIds: ['claude'],
        nativeSessionId: 'abc',
      }),
    ).toBe('claude --resume abc');
    expect(
      conversationResumeCommand({
        agentIds: ['claude'],
        nativeSessionId: null,
      }),
    ).toBeNull();
  });
});

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
    expect(cwdShortName('D:\\projects\\demo', t)).toBe('demo');
  });

  it('takes the last segment of a POSIX path', () => {
    expect(cwdShortName('/home/user/proj', t)).toBe('proj');
  });

  it('returns 未设目录 for null / undefined / empty', () => {
    expect(cwdShortName(null, t)).toBe('未设目录');
    expect(cwdShortName(undefined, t)).toBe('未设目录');
    expect(cwdShortName('', t)).toBe('未设目录');
    expect(cwdShortName('   ', t)).toBe('未设目录');
  });

  it('strips trailing separators on both styles', () => {
    expect(cwdShortName('D:\\projects\\demo\\', t)).toBe('demo');
    expect(cwdShortName('/home/user/proj/', t)).toBe('proj');
    expect(cwdShortName('C:\\\\', t)).toBe('C:');
    expect(cwdShortName('C:', t)).toBe('C:');
  });

  it('keeps POSIX root as /', () => {
    expect(cwdShortName('/', t)).toBe('/');
    expect(cwdShortName('///', t)).toBe('/');
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
    const groups = groupConversationsByDay([today, yesterday, week, earlier], now, t);

    expect(groups.map((g) => g.key)).toEqual(['today', 'yesterday', 'week', 'earlier']);
    expect(groups.map((g) => g.label)).toEqual(['今天', '昨天', '近 7 天', '更早']);
    expect(groups.map((g) => g.items.map((c) => c.id))).toEqual([['t'], ['y'], ['w'], ['e']]);
  });

  it('keeps input order inside a group and omits empty buckets', () => {
    const a = conv({ id: 'a', updatedAt: at(2026, 7, 16, 14) });
    const b = conv({ id: 'b', updatedAt: at(2026, 7, 16, 10) });
    const groups = groupConversationsByDay([a, b], now, t);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe('today');
    expect(groups[0].items.map((c) => c.id)).toEqual(['a', 'b']);
  });

  it('puts today-minus-6 in week and today-minus-7 in earlier', () => {
    const sixDays = conv({ id: 's', updatedAt: at(2026, 7, 10, 12) });
    const sevenDays = conv({ id: 'v', updatedAt: at(2026, 7, 9, 12) });
    const groups = groupConversationsByDay([sixDays, sevenDays], now, t);
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

  it('returns hiddenAgents before envNotReady before unconfiguredAuth before noCwd before sendingElsewhere', () => {
    const blockers = sendBlockers({
      conversation: { ...base, cwd: null, agentIds: ['claude', 'kimi', 'pi', 'grok'] },
      hiddenIds: new Set<AgentId>(['kimi']),
      envNotReadyIds: new Set<AgentId>(['pi', 'kimi']),
      unconfiguredAuthIds: new Set<AgentId>(['grok']),
      sendingConversationId: 'other',
      sendingTitle: '别的会话',
    });
    expect(blockers.map((b) => b.kind)).toEqual([
      'hiddenAgents',
      'envNotReady',
      'unconfiguredAuth',
      'noCwd',
      'sendingElsewhere',
    ]);
    expect(blockers[0]).toEqual({ kind: 'hiddenAgents', agentIds: ['kimi'] });
    expect(blockers[1]).toEqual({ kind: 'envNotReady', agentIds: ['pi'] });
    expect(blockers[2]).toEqual({ kind: 'unconfiguredAuth', agentIds: ['grok'] });
  });

  it('does not list a hidden agent again as envNotReady or unconfiguredAuth', () => {
    const blockers = sendBlockers({
      conversation: { ...base, agentIds: ['kimi'] },
      hiddenIds: new Set<AgentId>(['kimi']),
      envNotReadyIds: new Set<AgentId>(['kimi']),
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

  it('keeps only the first selectable agent from the current session', () => {
    const active = conv({
      id: 'a',
      // grok 在 claude 前：与 catalog 序（claude, grok）可区分
      agentIds: ['kimi', 'grok', 'codex', 'claude'],
      cwd: '/tmp/app',
    });
    expect(newConversationDefaults(active, agents)).toEqual({
      agentIds: ['grok'],
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

describe('selectConversationAgent', () => {
  it('replaces the current agent with the clicked one', () => {
    expect(
      selectConversationAgent({
        currentIds: ['grok'],
        nextId: 'codex',
        allowDangerous: false,
      }),
    ).toEqual({ agentIds: ['codex'] });
  });

  it('is a no-op when clicking the already selected agent', () => {
    expect(
      selectConversationAgent({
        currentIds: ['claude'],
        nextId: 'claude',
        allowDangerous: true,
      }),
    ).toBeNull();
  });

  it('clears auto-approve when switching to an agent that cannot skip confirms', () => {
    expect(
      selectConversationAgent({
        currentIds: ['claude'],
        nextId: 'kimi',
        allowDangerous: true,
      }),
    ).toEqual({ agentIds: ['kimi'], allowDangerous: false });
  });

  it('keeps auto-approve when switching to an agent that honors it', () => {
    expect(
      selectConversationAgent({
        currentIds: ['claude'],
        nextId: 'pi',
        allowDangerous: true,
      }),
    ).toEqual({ agentIds: ['pi'] });
  });
});

describe('autoApproveEffect', () => {
  it('matches headless adapter flags, not TUI capability labels', () => {
    expect(autoApproveEffect('claude')).toBe('skip');
    expect(autoApproveEffect('codex')).toBe('skip');
    expect(autoApproveEffect('grok')).toBe('skip');
    expect(autoApproveEffect('workbuddy')).toBe('skip');
    expect(autoApproveEffect('cursor')).toBe('skip');
    expect(autoApproveEffect('pi')).toBe('project-trust');
    expect(autoApproveEffect('kimi')).toBe('none');
    expect(autoApproveEffect('dsh')).toBe('none');
    expect(autoApproveEffect(null)).toBe('none');
  });

  it('only treats stored allowDangerous as active when the agent can honor it', () => {
    expect(autoApproveActive(true, 'claude')).toBe(true);
    expect(autoApproveActive(true, 'pi')).toBe(true);
    expect(autoApproveActive(true, 'kimi')).toBe(false);
    expect(autoApproveActive(false, 'claude')).toBe(false);
  });

  it('uses honest footer and confirm copy per effect', () => {
    expect(autoApproveFooter(t, false, 'claude').warning).toBe(false);
    expect(autoApproveFooter(t, true, 'claude')).toEqual({
      text: '自动批准已开启 · Agent 将不经确认修改文件',
      warning: true,
    });
    expect(autoApproveFooter(t, true, 'pi').text).toContain('仅信任项目文件');
    expect(autoApproveFooter(t, true, 'kimi').text).toContain('不会生效');
    expect(autoApproveHint(t, 'none')).toContain('无法跳过确认');
    expect(autoApproveConfirmCopy(t, 'project-trust')).toContain('不会完全跳过');
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
    expect(conversationTitle(t, '')).toBe('新对话');
    expect(conversationTitle(t, '   ')).toBe('新对话');
    expect(conversationTitle(t, '修复登录')).toBe('修复登录');
  });
});

describe('blockerCopy', () => {
  it('returns copy for each blocker kind', () => {
    expect(blockerCopy(t, { kind: 'hiddenAgents', agentIds: ['claude'] })).toEqual({
      text: '会话包含已隐藏 Agent，暂不能发送',
      primaryAction: '去 Agents 页',
    });
    expect(blockerCopy(t, { kind: 'envNotReady', agentIds: ['pi'] })).toEqual({
      text: '会话包含运行环境未就绪的 Agent，暂不能发送',
      primaryAction: '去 Agents 页',
    });
    expect(blockerCopy(t, { kind: 'unconfiguredAuth', agentIds: ['grok'] })).toEqual({
      text: '会话包含未配置授权的 Agent，暂不能发送',
      primaryAction: '去 Connections 页',
    });
    expect(blockerCopy(t, { kind: 'noCwd' })).toEqual({
      text: '未设置工作目录 — Agent 需要在指定目录内工作',
      primaryAction: '设置工作目录',
    });
    expect(
      blockerCopy(t, { kind: 'sendingElsewhere', conversationId: 'x', title: '对比方案' }),
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
    expect(blockerPrimaryTarget({ kind: 'envNotReady' })).toBe('agents');
    expect(blockerPrimaryTarget({ kind: 'unconfiguredAuth' })).toBe('connections');
    expect(blockerPrimaryTarget({ kind: 'noCwd' })).toBe('pick-directory');
    expect(blockerPrimaryTarget({ kind: 'sendingElsewhere' })).toBe('settings');
  });
});

describe('connectionPickerCaption', () => {
  it('returns the primary-agent caption only for multi-select', () => {
    expect(connectionPickerCaption(t, { agentIds: ['claude'] })).toBeNull();
    expect(
      connectionPickerCaption(t, { agentIds: ['claude', 'codex'], primaryAgent: 'claude' }),
    ).toBe(`仅作用于首位 Agent（${agentDisplayName('claude')}）`);
  });
});

describe('agentPickerLabel', () => {
  it('labels the first selected agent', () => {
    expect(agentPickerLabel(t, null)).toBe('选择 Agent');
    expect(agentPickerLabel(t, conv({ id: '1', agentIds: ['claude'] }))).toBe(
      agentDisplayName('claude'),
    );
    expect(agentPickerLabel(t, conv({ id: '2', agentIds: ['claude', 'codex'] }))).toBe(
      agentDisplayName('claude'),
    );
  });
});

describe('messageStatusLabel', () => {
  it('returns null for success statuses', () => {
    expect(messageStatusLabel(t, 'ok')).toBeNull();
    expect(messageStatusLabel(t, 'done')).toBeNull();
    expect(messageStatusLabel(t, 'success')).toBeNull();
  });

  it('uses process phase while running', () => {
    expect(messageStatusLabel(t, 'running', processView('queued'))).toBe('排队中');
    expect(messageStatusLabel(t, 'running', processView('starting'))).toBe('启动中');
    expect(messageStatusLabel(t, 'running', processView('running'))).toBe('生成中');
    expect(messageStatusLabel(t, 'running')).toBe('生成中');
  });

  it('maps terminal and unknown statuses', () => {
    expect(messageStatusLabel(t, 'failed')).toBe('失败');
    expect(messageStatusLabel(t, 'cancelled')).toBe('已取消');
    expect(messageStatusLabel(t, 'timeout')).toBe('超时');
    expect(messageStatusLabel(t, 'weird')).toBe('weird');
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

  it('isChatAgentSelectable only treats Pi envReady=false as blocked', () => {
    expect(isChatAgentSelectable(status('claude', true))).toBe(true);
    expect(isChatAgentSelectable(status('claude', true, false, { envReady: true }))).toBe(true);
    expect(isChatAgentSelectable(status('claude', true, false, { envReady: false }))).toBe(true);
    expect(isChatAgentSelectable(status('pi', true, false, { envReady: false }))).toBe(false);
    expect(isChatAgentSelectable(status('pi', true, false, { envReady: true }))).toBe(true);
  });

  it('omits hidden and uninstalled agents, and parks no-auth at the end as unselectable', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['claude', 'codex', 'kimi', 'grok', 'pi'],
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
    expect(rows.map((r) => r.id)).toEqual(['claude', 'pi', 'kimi']);
    expect(rows.map((r) => r.selectable)).toEqual([true, true, false]);
    expect(rows.map((r) => r.reason)).toEqual([null, null, 'noAuth']);
  });

  it('keeps envReady-false Pi unselectable; Claude envReady-false stays selectable', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['claude', 'pi'],
      agentStatus: [
        status('claude', true, false, { envReady: false }),
        status('pi', true, false, { envReady: false }),
      ],
    });
    expect(rows).toEqual([
      { id: 'claude', selectable: true, reason: null },
      { id: 'pi', selectable: false, reason: 'envNotReady' },
    ]);
  });

  it('prefers envNotReady over noAuth when both would apply', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['pi'],
      agentStatus: [
        status('pi', true, false, {
          envReady: false,
          effectiveKind: 'none',
          authStatus: 'none',
          authHealth: 'missing',
        }),
      ],
    });
    expect(rows).toEqual([{ id: 'pi', selectable: false, reason: 'envNotReady' }]);
  });

  it('does not keep a selected hidden or uninstalled agent in the picker', () => {
    const rows = chatAgentPickerRows({
      catalogIds: ['claude', 'kimi', 'codex'],
      agentStatus: [status('claude', true), status('kimi', false), status('codex', true, true)],
    });
    expect(rows.map((r) => r.id)).toEqual(['claude']);
  });
});

describe('chatAgentPickerEmptyKind', () => {
  it('is null when the picker has rows', () => {
    expect(chatAgentPickerEmptyKind({ agentsReady: true, rowCount: 2 })).toBeNull();
    expect(chatAgentPickerEmptyKind({ agentsReady: false, rowCount: 1 })).toBeNull();
  });

  it('does not treat an unreadied empty list as none installed', () => {
    expect(chatAgentPickerEmptyKind({ agentsReady: false, rowCount: 0 })).toBe('loading');
    expect(chatAgentPickerEmptyCopy(t, 'loading')).toEqual({
      text: '正在检测已安装的 Agent…',
      action: null,
    });
  });

  it('uses a single ready-empty copy for hidden-or-uninstalled', () => {
    expect(chatAgentPickerEmptyKind({ agentsReady: true, rowCount: 0 })).toBe('none');
    expect(chatAgentPickerEmptyCopy(t, 'none')).toEqual({
      text: '没有可选择的 Agent',
      action: '去 Agents 页',
    });
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
    const view = chatConnectionPickerView(t, {
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
    const view = chatConnectionPickerView(t, {
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
    const view = chatConnectionPickerView(t, {
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
    const view = chatConnectionPickerView(t, {
      primaryAgent: 'grok',
      status: grok,
      currentProviderName: 'stale-api',
    });
    expect(view.kind).toBe('account');
    expect(view.label).toBe('已登录');
    expect(view.emptyHint).toBeNull();
  });

  it('treats live API credentials without a current provider as API, not a login row', () => {
    const view = chatConnectionPickerView(t, {
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
    const view = chatConnectionPickerView(t, {
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
    const view = chatConnectionPickerView(t, {
      primaryAgent: 'claude',
      switching: true,
      status: status('claude', true),
      currentProviderName: 'official',
    });
    expect(view.label).toBe('切换中…');
    expect(view.subtitle).toBeNull();
  });
});

function oauthAccount(partial: Partial<Account> & Pick<Account, 'id'>): Account {
  return {
    agentId: 'codex',
    kind: 'oauth',
    label: partial.label ?? partial.email ?? partial.id,
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function providerRow(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'codex',
    preset: 'custom',
    configText: '{}',
    configFormat: 'toml',
    isCurrent: false,
    ...partial,
  };
}

describe('chatConnectionOptions', () => {
  it('labels official oauth with email / 官方登录, not 本机路由', () => {
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: 'user@openai.com',
          label: 'codex-live-1',
          isCurrent: true,
        }),
      ],
      providers: [],
      connectionKind: 'account',
    });
    expect(options).toHaveLength(1);
    expect(options[0]).toMatchObject({
      kind: 'account',
      id: 'codex-live-1',
      title: 'user@openai.com',
      subtitle: '官方登录',
      isCurrent: true,
    });
    expect(options[0].title).not.toContain('本机路由');
    expect(options[0].subtitle).not.toContain('本机路由');
  });

  it('labels leftover generated providers 本机路由, never 官方登录', () => {
    const leftover = providerRow({
      id: 'agenthub_grok_bridge',
      name: 'AgentHub Grok 本机路由',
      configText: 'model_provider = "agenthub_grok_bridge"\n[model_providers.agenthub_grok_bridge]\nbase_url = "http://127.0.0.1:32123/v1"',
      isCurrent: true,
    });
    expect(isLeftoverLocalRouteProvider(leftover)).toBe(true);
    const options = chatConnectionOptions(t, {
      accounts: [],
      providers: [leftover],
      connectionKind: 'api',
    });
    expect(options).toHaveLength(1);
    expect(options[0].kind).toBe('provider');
    expect(options[0].title).toBe('本机路由');
    expect(options[0].subtitle).not.toBe('官方登录');
    expect(options[0].title).not.toBe('官方登录');
  });

  it('lists official oauth and leftover 本机路由 as separate options', () => {
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: 'user@openai.com',
          isCurrent: true,
        }),
      ],
      providers: [
        providerRow({
          id: 'agenthub_codex_bridge',
          name: 'agenthub_codex_bridge',
          configText: 'base_url = "http://127.0.0.1:32123/v1"',
          isCurrent: true,
        }),
      ],
      connectionKind: 'account',
    });
    const oauth = options.find((row) => row.kind === 'account');
    const leftover = options.find((row) => row.kind === 'provider');
    expect(oauth).toMatchObject({
      title: 'user@openai.com',
      subtitle: '官方登录',
      isCurrent: false,
    });
    expect(leftover).toMatchObject({
      title: '本机路由',
      isCurrent: true,
    });
    expect(options.map((row) => row.kind)).toEqual(['account', 'provider']);
  });

  it('falls back to account.label when oauth has no email', () => {
    const options = chatConnectionOptions(t, {
      accounts: [oauthAccount({ id: 'codex-live-2', label: 'ChatGPT Plus' })],
      providers: [],
    });
    expect(options[0].title).toBe('ChatGPT Plus');
    expect(options[0].subtitle).toBe('官方登录');
  });

  it('makes official oauth clickable when leftover local-route is current', () => {
    const leftover = providerRow({
      id: 'agenthub_grok_bridge',
      name: 'AgentHub Grok 本机路由',
      configText: 'base_url = "http://127.0.0.1:43121/v1"',
      isCurrent: true,
    });
    expect(leftoverProviderIsCurrent([leftover])).toBe(true);
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: '41375197@qq.com',
          isCurrent: true,
        }),
      ],
      providers: [leftover],
      connectionKind: 'api',
    });
    const oauth = options.find((row) => row.kind === 'account');
    const route = options.find((row) => row.kind === 'provider');
    expect(oauth).toMatchObject({
      title: '41375197@qq.com',
      isCurrent: false,
    });
    expect(route).toMatchObject({
      title: '本机路由',
      isCurrent: true,
    });
    expect(oauth?.title).not.toContain('本机路由');
  });

  it('switch-back marks official current and leftover not current', () => {
    const leftover = providerRow({
      id: 'agenthub_grok_bridge',
      name: 'AgentHub Grok 本机路由',
      configText: 'base_url = "http://127.0.0.1:43121/v1"',
      isCurrent: false,
    });
    expect(leftoverProviderIsCurrent([leftover])).toBe(false);
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: '41375197@qq.com',
          isCurrent: true,
        }),
      ],
      providers: [leftover],
      connectionKind: 'account',
    });
    const oauth = options.find((row) => row.kind === 'account');
    const route = options.find((row) => row.kind === 'provider');
    expect(oauth).toMatchObject({
      title: '41375197@qq.com',
      subtitle: '官方登录',
      isCurrent: true,
    });
    expect(route).toMatchObject({
      title: '本机路由',
      isCurrent: false,
    });
    expect(oauth?.title).not.toContain('本机路由');
    expect(oauth?.subtitle).not.toContain('本机路由');
    const chip = chatConnectionPickerView(t, {
      primaryAgent: 'codex',
      status: status('codex', true, false, {
        effectiveKind: 'account',
        effectiveLabel: '41375197@qq.com',
      }),
    });
    expect(chip.label).toBe('41375197@qq.com');
    expect(chip.label).not.toContain('本机路由');
  });

  it('dedupes official oauth rows with the same email', () => {
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: '41375197@qq.com',
          isCurrent: false,
        }),
        oauthAccount({
          id: 'codex-live-2',
          email: '41375197@qq.com',
          isCurrent: true,
        }),
      ],
      providers: [
        providerRow({
          id: 'agenthub_grok_bridge',
          name: 'AgentHub Grok 本机路由',
          configText: 'base_url = "http://127.0.0.1:43121/v1"',
          isCurrent: true,
        }),
      ],
      connectionKind: 'api',
    });
    const official = options.filter((row) => row.kind === 'account');
    expect(official).toHaveLength(1);
    expect(official[0].id).toBe('codex-live-2');
    expect(official[0].title).toBe('41375197@qq.com');
    expect(official[0].subtitle).toBe('官方登录');
    expect(official[0].isCurrent).toBe(false);
    expect(options.filter((row) => row.kind === 'provider')).toHaveLength(1);
  });

  it('collapses leftover providers to the current leftover', () => {
    const options = chatConnectionOptions(t, {
      accounts: [
        oauthAccount({
          id: 'codex-live-1',
          email: 'user@openai.com',
          isCurrent: true,
        }),
      ],
      providers: [
        providerRow({
          id: 'agenthub_codex_bridge_old',
          name: 'AgentHub Codex 本机路由',
          configText: 'base_url = "http://127.0.0.1:32123/v1"',
          isCurrent: false,
          updatedAt: '2026-08-19T00:00:00.000Z',
        }),
        providerRow({
          id: 'agenthub_codex_bridge_current',
          name: 'AgentHub Codex 本机路由',
          configText: 'base_url = "http://127.0.0.1:43121/v1"',
          isCurrent: true,
          updatedAt: '2026-01-01T00:00:00.000Z',
        }),
      ],
      connectionKind: 'api',
    });
    const leftovers = options.filter((row) => row.kind === 'provider');
    expect(leftovers).toHaveLength(1);
    expect(leftovers[0]).toMatchObject({
      id: 'agenthub_codex_bridge_current',
      title: '本机路由',
      subtitle: null,
      isCurrent: true,
    });
    expect(options.find((row) => row.kind === 'account')).toMatchObject({
      title: 'user@openai.com',
      isCurrent: false,
    });
  });

  it('collapses leftover providers to the latest updatedAt when none is current', () => {
    const options = chatConnectionOptions(t, {
      accounts: [],
      providers: [
        providerRow({
          id: 'agenthub_bridge_a',
          name: 'AgentHub 本机路由',
          configText: 'base_url = "http://127.0.0.1:32123/v1"',
          updatedAt: '2026-08-01T00:00:00.000Z',
        }),
        providerRow({
          id: 'openai-official',
          name: 'OpenAI',
          configText: 'model = "gpt-4"',
        }),
        providerRow({
          id: 'agenthub_bridge_b',
          name: 'AgentHub 本机路由',
          configText: 'base_url = "http://127.0.0.1:43121/v1"',
          updatedAt: '2026-08-10T00:00:00.000Z',
        }),
      ],
    });
    expect(options.map((row) => row.id)).toEqual(['openai-official', 'agenthub_bridge_b']);
    expect(options[1]).toMatchObject({
      kind: 'provider',
      id: 'agenthub_bridge_b',
      title: '本机路由',
      subtitle: null,
      isCurrent: false,
    });
  });
});

describe('leftoverBindTicketId', () => {
  it('returns account:src-1 when a profile generatedProviderId matches', () => {
    expect(
      leftoverBindTicketId('gen-1', [
        { generatedProviderId: 'other', sourceKind: 'provider', sourceId: 'p-1' },
        { generatedProviderId: 'gen-1', sourceKind: 'account', sourceId: 'src-1' },
      ]),
    ).toBe('account:src-1');
  });

  it('returns null when no profile matches', () => {
    expect(
      leftoverBindTicketId('gen-1', [
        { generatedProviderId: 'other', sourceKind: 'account', sourceId: 'src-1' },
        { generatedProviderId: null, sourceKind: 'provider', sourceId: 'p-1' },
      ]),
    ).toBeNull();
  });
});

describe('clampComposerTextareaHeight', () => {
  it('keeps a short draft at the min row height', () => {
    expect(clampComposerTextareaHeight(0)).toBe(COMPOSER_TEXTAREA_MIN_PX);
    expect(clampComposerTextareaHeight(-12)).toBe(COMPOSER_TEXTAREA_MIN_PX);
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MIN_PX - 1)).toBe(COMPOSER_TEXTAREA_MIN_PX);
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MIN_PX)).toBe(COMPOSER_TEXTAREA_MIN_PX);
  });

  it('grows with content until the max, then caps', () => {
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MIN_PX + 1)).toBe(COMPOSER_TEXTAREA_MIN_PX + 1);
    expect(clampComposerTextareaHeight(120)).toBe(120);
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MAX_PX - 1)).toBe(COMPOSER_TEXTAREA_MAX_PX - 1);
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MAX_PX)).toBe(COMPOSER_TEXTAREA_MAX_PX);
    expect(clampComposerTextareaHeight(COMPOSER_TEXTAREA_MAX_PX + 80)).toBe(COMPOSER_TEXTAREA_MAX_PX);
  });
});

describe('composerTextareaOverflowY', () => {
  it('scrolls only after the cap', () => {
    expect(composerTextareaOverflowY(0)).toBe('hidden');
    expect(composerTextareaOverflowY(COMPOSER_TEXTAREA_MIN_PX)).toBe('hidden');
    expect(composerTextareaOverflowY(COMPOSER_TEXTAREA_MAX_PX)).toBe('hidden');
    expect(composerTextareaOverflowY(COMPOSER_TEXTAREA_MAX_PX + 1)).toBe('auto');
  });
});

describe('composerTextareaMeasuredStyle', () => {
  it('emits the clamped height and overflow the textarea should apply', () => {
    expect(composerTextareaMeasuredStyle(24)).toEqual({
      height: `${COMPOSER_TEXTAREA_MIN_PX}px`,
      overflowY: 'hidden',
    });
    expect(composerTextareaMeasuredStyle(120)).toEqual({
      height: '120px',
      overflowY: 'hidden',
    });
    expect(composerTextareaMeasuredStyle(COMPOSER_TEXTAREA_MAX_PX + 40)).toEqual({
      height: `${COMPOSER_TEXTAREA_MAX_PX}px`,
      overflowY: 'auto',
    });
  });
});

describe('composerUsesCssFieldSizing', () => {
  it('is false without CSS.supports', () => {
    expect(composerUsesCssFieldSizing(null)).toBe(false);
    expect(composerUsesCssFieldSizing({})).toBe(false);
  });

  it('follows CSS.supports for field-sizing: content', () => {
    expect(composerUsesCssFieldSizing({ supports: () => true })).toBe(true);
    expect(
      composerUsesCssFieldSizing({
        supports: (property, value) => property === 'field-sizing' && value === 'content',
      }),
    ).toBe(true);
    expect(composerUsesCssFieldSizing({ supports: () => false })).toBe(false);
  });
});

describe('chat transcript / composer surfaces', () => {
  it('uses canvas when empty so the transcript matches the composer chrome', () => {
    expect(chatTranscriptSurfaceClass(false)).toBe('bg-canvas');
    expect(chatComposerChromeClass(false)).toBe('shrink-0 bg-canvas pb-4 pt-2');
    expect(chatComposerChromeClass(false)).not.toContain('border-t');
  });

  it('uses panel for the transcript once messages exist, matching the input shell', () => {
    expect(chatTranscriptSurfaceClass(true)).toBe('bg-panel');
    expect(chatComposerChromeClass(true)).toBe(
      'shrink-0 border-t border-border/60 bg-canvas pb-4 pt-2',
    );
  });
});
