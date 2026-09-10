import { describe, expect, it } from 'vitest';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { agentDisplayName } from '@/config/agents';
import { createTranslator } from '@/lib/i18n';
import type { BindingView, TicketView, TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentKey, AgentStatus, ChatMessage, Conversation, Provider } from '@/lib/types';
import type { AgentProcessView } from '@/lib/chat-process';
import type { TurnGroup } from './chat-format';
import {
  agentHasConfiguredAuth,
  agentPickerLabel,
  blockerCopy,
  blockerPrimaryTarget,
  chatAgentPickerEmptyCopy,
  chatAgentPickerEmptyKind,
  chatEscapeShouldCancel,
  chatKeyTargetIsField,
  chatModKShouldFocusHistory,
  chatModNShouldStartNewChat,
  chatPageShortcutAction,
  chatQuestionShouldOpenShortcuts,
  composerEnterShouldSend,
  composerNativeEditChord,
  dialogEnterShouldConfirm,
  chatAgentPickerRows,
  chatConnectionKind,
  chatConnectionOptions,
  chatConnectionSwitchAction,
  chatShowsUnimportedCurrent,
  clampComposerTextareaHeight,
  COMPOSER_TEXTAREA_MAX_PX,
  COMPOSER_TEXTAREA_MIN_PX,
  composerTextareaMeasuredStyle,
  composerTextareaOverflowY,
  composerUsesCssFieldSizing,
  chatMainColumnClass,
  chatStageClass,
  chatTranscriptSurfaceClass,
  chatConnectionPickerView,
  connectionPickerCaption,
  isLeftoverLocalRouteProvider,
  leftoverProviderIsCurrent,
  conversationResumeCommand,
  conversationAgentLine,
  conversationRailHint,
  conversationRailHintView,
  conversationRailMarkColor,
  conversationRailSelectedFill,
  conversationSemanticTitle,
  conversationTitle,
  titleFromPrompt,
  conversationCwdMissing,
  canRebindConversationCwd,
  cwdShortName,
  isBlankConversationDraft,
  draftForFocusedConversation,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  liveSendingIds,
  incomingSendingIds,
  busyAgentsForSends,
  messageStatusLabel,
  newConversationDefaults,
  autoApproveActive,
  autoApproveConfirmCopy,
  autoApproveEffect,
  autoApproveFooter,
  autoApproveHint,
  selectConversationAgent,
  singleAgentConversationPatch,
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
    expect(
      conversationResumeCommand({
        agentIds: ['kiro'],
        nativeSessionId: 'sess-1',
      }),
    ).toBe('kiro-cli chat --resume-id sess-1');
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
  agentId: AgentKey,
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

describe('isBlankConversationDraft', () => {
  it('treats an untitled row without an official session as a draft', () => {
    expect(isBlankConversationDraft({ title: '', nativeSessionId: null })).toBe(true);
    expect(isBlankConversationDraft({ title: '  ', nativeSessionId: undefined })).toBe(true);
    expect(isBlankConversationDraft({ title: 'hi', nativeSessionId: null })).toBe(false);
    expect(isBlankConversationDraft({ title: '', nativeSessionId: 'sess-1' })).toBe(false);
  });
});

describe('conversationAgentLine', () => {
  it('names one or two agents and then counts extras', () => {
    expect(conversationAgentLine(['claude'])).toBe(agentDisplayName('claude'));
    expect(conversationAgentLine(['claude', 'pi'])).toBe(
      `${agentDisplayName('claude')} · ${agentDisplayName('pi')}`,
    );
    expect(conversationAgentLine(['claude', 'pi', 'codex'])).toBe(
      `${agentDisplayName('claude')} +2`,
    );
  });
});

describe('conversationRailHint', () => {
  it('joins directory, time, and extra session facts without Agent names', () => {
    expect(
      conversationRailHint(
        {
          title: '',
          agentIds: ['claude'],
          cwd: 'D:\\demo',
          updatedAt: new Date().toISOString(),
          nativeSessionId: null,
        },
        t,
      ),
    ).toBe('D:\\demo · 刚刚 · 草稿');
    expect(
      conversationRailHint(
        {
          title: '修登录',
          agentIds: ['claude'],
          cwd: '',
          updatedAt: new Date().toISOString(),
          nativeSessionId: 'sess-1',
        },
        t,
      ),
    ).toBe('修登录 · 未设目录 · 刚刚 · 已关联官方会话 sess-1');
    expect(
      conversationRailHint(
        {
          title: '请在 /workspace/src/app.ts 检查问题',
          agentIds: ['codex'],
          cwd: '/workspace/demo-project',
          updatedAt: new Date().toISOString(),
          nativeSessionId: 'sess-1',
        },
        t,
      ),
    ).toContain('/workspace/src/app.ts');
    expect(
      conversationRailHint(
        {
          title: '请在 /workspace/src/app.ts 检查问题',
          agentIds: ['codex'],
          cwd: '/workspace/demo-project',
          updatedAt: new Date().toISOString(),
          nativeSessionId: null,
        },
        t,
      ),
    ).not.toMatch(/^\/workspace/);
  });

  it('exposes the complete stored title for hover, never an ellipsized clip', () => {
    const title =
      'Please create or edit /workspace/src/pages/chat/ChatSessionRail.tsx to add a hover title';
    const hint = conversationRailHintView(
      {
        title,
        cwd: '/workspace/demo-project',
        updatedAt: new Date().toISOString(),
        nativeSessionId: null,
      },
      t,
    );
    expect(hint.title).toBe(title);
    expect(hint.title).not.toMatch(/…|\.\.\./);
    expect(conversationSemanticTitle(title)).not.toBe(title);
    expect(conversationSemanticTitle(title)).toMatch(/…/);
    expect(hint.meta).toContain('/workspace/demo-project');
  });
});

describe('conversationRailMarkColor', () => {
  it('uses the first Agent brand, then the nav accent', () => {
    expect(conversationRailMarkColor(['claude'])).toBe('var(--agent-claude)');
    expect(conversationRailMarkColor(['claude', 'pi'])).toBe('var(--agent-claude)');
    expect(conversationRailMarkColor([])).toBe('var(--accent)');
  });
});

describe('conversationRailSelectedFill', () => {
  it('washes the Agent mark onto the canvas', () => {
    expect(conversationRailSelectedFill(['claude'])).toBe(
      'color-mix(in srgb, var(--agent-claude) 28%, var(--bg-canvas))',
    );
    expect(conversationRailSelectedFill([])).toBe(
      'color-mix(in srgb, var(--accent) 28%, var(--bg-canvas))',
    );
  });
});

describe('conversationCwdMissing', () => {
  it('is only true when a stored path is marked gone', () => {
    expect(conversationCwdMissing({ cwd: '/tmp', cwdMissing: true })).toBe(true);
    expect(conversationCwdMissing({ cwd: '/tmp', cwdMissing: false })).toBe(false);
    expect(conversationCwdMissing({ cwd: null, cwdMissing: true })).toBe(false);
    expect(canRebindConversationCwd({ cwd: '/tmp', cwdMissing: true }, true)).toBe(true);
    expect(canRebindConversationCwd({ cwd: '/tmp', cwdMissing: false }, true)).toBe(false);
    expect(canRebindConversationCwd({ cwd: null }, false)).toBe(true);
  });
});

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

  it('returns hiddenAgents before envNotReady before unconfiguredAuth before noCwd', () => {
    const blockers = sendBlockers({
      conversation: { ...base, cwd: null, agentIds: ['claude', 'kimi', 'pi', 'grok'] },
      hiddenIds: new Set<AgentKey>(['kimi']),
      envNotReadyIds: new Set<AgentKey>(['pi', 'kimi']),
      unconfiguredAuthIds: new Set<AgentKey>(['grok']),
    });
    expect(blockers.map((b) => b.kind)).toEqual([
      'hiddenAgents',
      'envNotReady',
      'unconfiguredAuth',
      'noCwd',
    ]);
    expect(blockers[0]).toEqual({ kind: 'hiddenAgents', agentIds: ['kimi'] });
    expect(blockers[1]).toEqual({ kind: 'envNotReady', agentIds: ['pi'] });
    expect(blockers[2]).toEqual({ kind: 'unconfiguredAuth', agentIds: ['grok'] });
  });

  it('does not list a hidden agent again as envNotReady or unconfiguredAuth', () => {
    const blockers = sendBlockers({
      conversation: { ...base, agentIds: ['kimi'] },
      hiddenIds: new Set<AgentKey>(['kimi']),
      envNotReadyIds: new Set<AgentKey>(['kimi']),
      unconfiguredAuthIds: new Set<AgentKey>(['kimi']),
    });
    expect(blockers.map((b) => b.kind)).toEqual(['hiddenAgents']);
  });

  it('does not treat an empty draft as a blocker', () => {
    expect(
      sendBlockers({
        conversation: base,
        hiddenIds: new Set(),
      }),
    ).toEqual([]);
  });

  it('blocks send when Agent status has not loaded', () => {
    expect(
      sendBlockers({
        conversation: base,
        hiddenIds: new Set(),
        agentsReady: false,
      }),
    ).toEqual([{ kind: 'statusUnknown' }]);
  });
});

describe('incomingSendingIds', () => {
  it('treats null, undefined, and non-arrays as empty', () => {
    expect(incomingSendingIds(null)).toEqual([]);
    expect(incomingSendingIds(undefined)).toEqual([]);
    expect(incomingSendingIds(1)).toEqual([]);
  });

  it('keeps a legacy single id and ignores empty strings', () => {
    expect(incomingSendingIds('sess-1')).toEqual(['sess-1']);
    expect(incomingSendingIds('')).toEqual([]);
    expect(incomingSendingIds(['a', '', 'a', 1, 'b'])).toEqual(['a', 'b']);
  });
});

describe('liveSendingIds', () => {
  it('drops ids that are no longer in the conversation list', () => {
    expect(liveSendingIds(['a', 'gone', 'b'], [conv({ id: 'b' }), conv({ id: 'a' })])).toEqual([
      'a',
      'b',
    ]);
  });

  it('treats a missing sending list as empty', () => {
    expect(liveSendingIds(null, [conv({ id: 'a' })])).toEqual([]);
  });
});

describe('busyAgentsForSends', () => {
  it('collects the primary agent of each in-flight conversation', () => {
    expect(
      busyAgentsForSends(
        [
          conv({ id: 'a', agentIds: ['claude'] }),
          conv({ id: 'b', agentIds: ['codex'] }),
          conv({ id: 'c', agentIds: ['claude'] }),
        ],
        ['a', 'c'],
      ),
    ).toEqual(new Set<AgentKey>(['claude']));
  });
});

describe('draftForFocusedConversation', () => {
  it('saves the leaving draft and restores the focused one', () => {
    const drafts = new Map<string, string>([['b', 'hello B']]);
    expect(draftForFocusedConversation(drafts, 'a', 'b', 'hello A')).toBe('hello B');
    expect(drafts.get('a')).toBe('hello A');
  });

  it('returns an empty draft when the focused session has none', () => {
    const drafts = new Map<string, string>();
    expect(draftForFocusedConversation(drafts, 'a', 'b', 'keep A')).toBe('');
    expect(drafts.get('a')).toBe('keep A');
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

describe('singleAgentConversationPatch', () => {
  it('returns null for an already single-agent conversation', () => {
    expect(singleAgentConversationPatch(['claude'])).toBeNull();
  });

  it('returns null for an empty agent list', () => {
    expect(singleAgentConversationPatch([])).toBeNull();
  });

  it('collapses a legacy multi-agent conversation to the first agent only', () => {
    expect(singleAgentConversationPatch(['claude', 'codex', 'grok'])).toEqual({
      agentIds: ['claude'],
    });
  });
});

describe('autoApproveEffect', () => {
  it('matches headless adapter flags, not TUI capability labels', () => {
    expect(autoApproveEffect('claude')).toBe('skip');
    expect(autoApproveEffect('codex')).toBe('skip');
    expect(autoApproveEffect('grok')).toBe('skip');
    expect(autoApproveEffect('workbuddy')).toBe('skip');
    expect(autoApproveEffect('cursor')).toBe('skip');
    expect(autoApproveEffect('kiro')).toBe('skip');
    expect(autoApproveEffect('pi')).toBe('project-trust');
    expect(autoApproveEffect('kimi')).toBe('none');
    expect(autoApproveEffect('dsh')).toBe('none');
    expect(autoApproveEffect(null)).toBe('none');
  });

  it('only treats stored allowDangerous as active when the agent can honor it', () => {
    expect(autoApproveActive(true, 'claude')).toBe(true);
    expect(autoApproveActive(true, 'kiro')).toBe(true);
    expect(autoApproveActive(true, 'pi')).toBe(true);
    expect(autoApproveActive(true, 'kimi')).toBe(false);
    expect(autoApproveActive(false, 'claude')).toBe(false);
  });

  it('uses honest footer and confirm copy per effect', () => {
    expect(autoApproveFooter(t, false, 'claude')).toEqual({ text: '', warning: false });
    expect(autoApproveFooter(t, true, 'claude')).toEqual({
      text: '自动批准已开启 · Agent 将不经确认修改文件',
      warning: true,
    });
    expect(autoApproveFooter(t, true, 'kiro').text).toContain('完全访问权限已开启');
    expect(autoApproveFooter(t, true, 'pi').text).toContain('仅信任项目文件');
    expect(autoApproveFooter(t, true, 'kimi').text).toContain('不会生效');
    expect(autoApproveHint(t, 'none')).toContain('无法跳过确认');
    expect(autoApproveHint(t, 'skip', 'kiro')).toContain('不再逐条询问');
    expect(autoApproveConfirmCopy(t, 'project-trust')).toContain('不会完全跳过');
    expect(autoApproveConfirmCopy(t, 'skip', 'kiro')).toContain('不再逐条询问');
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

  it('uses a semantic phrase instead of a path-first prompt clip', () => {
    expect(conversationSemanticTitle('Only modify /tmp/qa/ping.png')).toBe('Only modify');
    expect(conversationSemanticTitle('请在 /workspace/src/app.ts 检查问题')).toBe('检查问题');
    expect(conversationSemanticTitle('请帮我了解这个项目')).toBe('请帮我了解这个项目');
    expect(conversationSemanticTitle('/workspace/foo/bar.ts')).toBe('');
    expect(conversationTitle(t, 'Only modify /tmp/qa/ping.png')).toBe('Only modify');
    expect(conversationTitle(t, '请在 /workspace/AgentHub-pr332 修这个')).toBe('修这个');
    expect(conversationTitle(t, '/tmp/only-a-path')).toBe('新对话');
    expect(titleFromPrompt('请在 /workspace/src 检查问题')).toBe('检查问题');
    expect(titleFromPrompt('Only modify /tmp/foo')).not.toMatch(/\/tmp/);
  });
});

describe('blockerCopy', () => {
  it('returns copy for each blocker kind', () => {
    expect(blockerCopy(t, { kind: 'hiddenAgents', agentIds: ['claude'] })).toEqual({
      text: '会话包含已隐藏 Agent，暂不能发送',
      primaryAction: '去 Agent 页',
    });
    expect(blockerCopy(t, { kind: 'envNotReady', agentIds: ['pi'] })).toEqual({
      text: '会话包含运行环境未就绪的 Agent，暂不能发送',
      primaryAction: '去 Agent 页',
    });
    expect(blockerCopy(t, { kind: 'unconfiguredAuth', agentIds: ['grok'] })).toEqual({
      text: '会话包含未配置授权的 Agent，暂不能发送',
      primaryAction: '去连接页',
    });
    expect(blockerCopy(t, { kind: 'noCwd' })).toEqual({
      text: '未设置工作目录 — Agent 需要在指定目录内工作',
      primaryAction: '设置工作目录',
    });
  });
});

describe('blockerPrimaryTarget', () => {
  it('sends noCwd to the directory picker, not session settings', () => {
    expect(blockerPrimaryTarget({ kind: 'hiddenAgents' })).toBe('agents');
    expect(blockerPrimaryTarget({ kind: 'envNotReady' })).toBe('agents');
    expect(blockerPrimaryTarget({ kind: 'unconfiguredAuth' })).toBe('connections');
    expect(blockerPrimaryTarget({ kind: 'noCwd' })).toBe('pick-directory');
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
    expect(messageStatusLabel(t, 'running', processView('running'))).toBe('正在想');
    expect(messageStatusLabel(t, 'running', processView('running'), true)).toBe('正在写');
    expect(messageStatusLabel(t, 'running')).toBe('正在想');
  });

  it('maps terminal and unknown statuses', () => {
    expect(messageStatusLabel(t, 'failed')).toBe('失败');
    expect(messageStatusLabel(t, 'cancelled')).toBe('已取消');
    expect(messageStatusLabel(t, 'timeout')).toBe('超时');
    expect(messageStatusLabel(t, 'weird')).toBe('weird');
  });
});

describe('chatEscapeShouldCancel', () => {
  const idle = {
    key: 'Escape',
    sending: true,
    canceling: false,
    previewOpen: false,
    overlayOpen: false,
    defaultPrevented: false,
  };

  it('cancels an in-flight turn', () => {
    expect(chatEscapeShouldCancel(idle)).toBe(true);
  });

  it('yields to overlays, preview, IME, and a stop already in progress', () => {
    expect(chatEscapeShouldCancel({ ...idle, previewOpen: true })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, overlayOpen: true })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, defaultPrevented: true })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, composing: true })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, canceling: true })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, sending: false })).toBe(false);
    expect(chatEscapeShouldCancel({ ...idle, key: 'Enter' })).toBe(false);
  });
});

describe('dialogEnterShouldConfirm', () => {
  it('confirms delete on Enter, not Shift+Enter or IME', () => {
    expect(dialogEnterShouldConfirm({ key: 'Enter', shiftKey: false })).toBe(true);
    expect(dialogEnterShouldConfirm({ key: 'Enter', shiftKey: true })).toBe(false);
    expect(dialogEnterShouldConfirm({ key: 'Escape', shiftKey: false })).toBe(false);
    expect(dialogEnterShouldConfirm({ key: 'Enter', shiftKey: false, isComposing: true })).toBe(false);
    expect(
      dialogEnterShouldConfirm({
        key: 'Enter',
        shiftKey: false,
        nativeEvent: { keyCode: 229 },
      }),
    ).toBe(false);
  });
});

describe('composerEnterShouldSend', () => {
  it('sends on Enter and keeps Shift+Enter as a newline', () => {
    expect(composerEnterShouldSend({ key: 'Enter', shiftKey: false })).toBe(true);
    expect(composerEnterShouldSend({ key: 'Enter', shiftKey: true })).toBe(false);
    expect(composerEnterShouldSend({ key: 'a', shiftKey: false })).toBe(false);
  });

  it('does not send while the IME is composing', () => {
    expect(composerEnterShouldSend({ key: 'Enter', shiftKey: false, isComposing: true })).toBe(false);
    expect(
      composerEnterShouldSend({
        key: 'Enter',
        shiftKey: false,
        nativeEvent: { isComposing: true },
      }),
    ).toBe(false);
    expect(
      composerEnterShouldSend({
        key: 'Enter',
        shiftKey: false,
        nativeEvent: { keyCode: 229 },
      }),
    ).toBe(false);
  });
});

describe('chatModKShouldFocusHistory', () => {
  const base = {
    key: 'k',
    metaKey: false,
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
    overlayOpen: false,
  };

  it('focuses history search with Ctrl/Cmd+K', () => {
    expect(chatModKShouldFocusHistory(base)).toBe(true);
    expect(chatModKShouldFocusHistory({ ...base, ctrlKey: false, metaKey: true })).toBe(true);
  });

  it('yields to overlays, Shift, and Alt', () => {
    expect(chatModKShouldFocusHistory({ ...base, overlayOpen: true })).toBe(false);
    expect(chatModKShouldFocusHistory({ ...base, shiftKey: true })).toBe(false);
    expect(chatModKShouldFocusHistory({ ...base, altKey: true })).toBe(false);
    expect(chatModKShouldFocusHistory({ ...base, ctrlKey: false, metaKey: false })).toBe(false);
  });
});

describe('chatModNShouldStartNewChat', () => {
  const base = {
    key: 'n',
    metaKey: false,
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
    overlayOpen: false,
  };

  it('starts a new chat with Ctrl/Cmd+N', () => {
    expect(chatModNShouldStartNewChat(base)).toBe(true);
    expect(chatModNShouldStartNewChat({ ...base, ctrlKey: false, metaKey: true })).toBe(true);
    expect(chatModNShouldStartNewChat({ ...base, key: 'N' })).toBe(true);
    expect(
      chatModNShouldStartNewChat({ ...base, key: 'Unidentified', code: 'KeyN' }),
    ).toBe(true);
  });

  it('yields to overlays, Shift, and Alt', () => {
    expect(chatModNShouldStartNewChat({ ...base, overlayOpen: true })).toBe(false);
    expect(chatModNShouldStartNewChat({ ...base, shiftKey: true })).toBe(false);
    expect(chatModNShouldStartNewChat({ ...base, altKey: true })).toBe(false);
    expect(chatModNShouldStartNewChat({ ...base, ctrlKey: false, metaKey: false })).toBe(false);
    expect(chatModNShouldStartNewChat({ ...base, key: 'k' })).toBe(false);
    expect(
      chatModNShouldStartNewChat({ ...base, key: 'Unidentified', code: 'KeyK' }),
    ).toBe(false);
  });
});

describe('chatQuestionShouldOpenShortcuts', () => {
  const base = {
    key: '?',
    shiftKey: false,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    overlayOpen: false,
    typingInField: false,
  };

  it('opens the overview with ? when not typing', () => {
    expect(chatQuestionShouldOpenShortcuts(base)).toBe(true);
    expect(
      chatQuestionShouldOpenShortcuts({
        ...base,
        key: '?',
        code: undefined,
        shiftKey: false,
      }),
    ).toBe(true);
  });

  it('opens the overview from US Shift+/ when that is how ? is typed', () => {
    expect(
      chatQuestionShouldOpenShortcuts({ ...base, key: '/', shiftKey: true }),
    ).toBe(true);
    expect(
      chatQuestionShouldOpenShortcuts({
        ...base,
        key: 'Unidentified',
        code: 'Slash',
        shiftKey: true,
      }),
    ).toBe(true);
    expect(
      chatQuestionShouldOpenShortcuts({
        ...base,
        key: 'x',
        code: 'Slash',
        shiftKey: true,
      }),
    ).toBe(false);
  });

  it('yields to fields, overlays, and modifiers', () => {
    expect(chatQuestionShouldOpenShortcuts({ ...base, typingInField: true })).toBe(false);
    expect(chatQuestionShouldOpenShortcuts({ ...base, overlayOpen: true })).toBe(false);
    expect(chatQuestionShouldOpenShortcuts({ ...base, ctrlKey: true })).toBe(false);
    expect(chatQuestionShouldOpenShortcuts({ ...base, metaKey: true })).toBe(false);
    expect(chatQuestionShouldOpenShortcuts({ ...base, altKey: true })).toBe(false);
    expect(chatQuestionShouldOpenShortcuts({ ...base, key: '/' })).toBe(false);
  });
});

describe('chatPageShortcutAction', () => {
  const textarea = { tagName: 'TEXTAREA' } as unknown as EventTarget;
  const button = { tagName: 'BUTTON' } as unknown as EventTarget;
  const mods = {
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    overlayOpen: false,
  };

  it('leaves Ctrl/Cmd+A/C/X/V to the field', () => {
    for (const key of ['a', 'c', 'x', 'v'] as const) {
      expect(
        chatPageShortcutAction({
          ...mods,
          key,
          code: `Key${key.toUpperCase()}`,
          ctrlKey: true,
          target: textarea,
        }),
      ).toBeNull();
      expect(
        composerNativeEditChord({
          key,
          code: `Key${key.toUpperCase()}`,
          metaKey: false,
          ctrlKey: true,
          altKey: false,
          shiftKey: false,
        }),
      ).toBe(
        key === 'a' ? 'selectAll' : key === 'c' ? 'copy' : key === 'x' ? 'cut' : 'paste',
      );
    }
    expect(
      composerNativeEditChord({
        key: 'Unidentified',
        code: 'KeyA',
        metaKey: true,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
      }),
    ).toBe('selectAll');
    expect(
      composerNativeEditChord({
        key: 'a',
        code: 'KeyA',
        metaKey: false,
        ctrlKey: true,
        altKey: true,
        shiftKey: false,
      }),
    ).toBeNull();
  });

  it('starts a new chat from Ctrl+N even when the target is the composer textarea', () => {
    expect(
      chatPageShortcutAction({
        ...mods,
        key: 'n',
        code: 'KeyN',
        ctrlKey: true,
        target: textarea,
      }),
    ).toBe('newChat');
    expect(
      chatPageShortcutAction({
        ...mods,
        key: 'Unidentified',
        code: 'KeyN',
        ctrlKey: true,
        target: textarea,
      }),
    ).toBe('newChat');
  });

  it('keeps ? literal in the composer and opens the overview outside fields', () => {
    expect(chatPageShortcutAction({ ...mods, key: '?', target: textarea })).toBeNull();
    expect(chatPageShortcutAction({ ...mods, key: '?', target: button })).toBe('overview');
    expect(
      chatPageShortcutAction({
        ...mods,
        key: '/',
        code: 'Slash',
        shiftKey: true,
        target: button,
      }),
    ).toBe('overview');
    expect(
      chatPageShortcutAction({
        ...mods,
        key: 'Unidentified',
        code: 'Slash',
        shiftKey: true,
        target: button,
      }),
    ).toBe('overview');
    expect(
      chatPageShortcutAction({
        ...mods,
        key: '/',
        code: 'Slash',
        shiftKey: true,
        target: textarea,
      }),
    ).toBeNull();
  });
});

describe('chatKeyTargetIsField', () => {
  const fieldTarget = (partial: { tagName?: string; isContentEditable?: boolean }) =>
    partial as unknown as EventTarget;

  it('treats input, textarea, select, and contenteditable as fields', () => {
    expect(chatKeyTargetIsField(fieldTarget({ tagName: 'TEXTAREA' }))).toBe(true);
    expect(chatKeyTargetIsField(fieldTarget({ tagName: 'INPUT' }))).toBe(true);
    expect(chatKeyTargetIsField(fieldTarget({ tagName: 'SELECT' }))).toBe(true);
    expect(chatKeyTargetIsField(fieldTarget({ isContentEditable: true }))).toBe(true);
    expect(chatKeyTargetIsField(fieldTarget({ tagName: 'BUTTON' }))).toBe(false);
    expect(chatKeyTargetIsField(null)).toBe(false);
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
      action: '去 Agent 页',
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
    expect(view.manageLabel).toBe('去连接页管理');
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
    expect(view.manageLabel).toBe('去连接页管理');
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
    expect(view.manageLabel).toBe('去连接页添加');
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

  it('does not treat leftover 本机路由 as an unimported current login', () => {
    const view = chatConnectionPickerView(t, {
      primaryAgent: 'codex',
      leftoverCurrent: true,
      walletReady: true,
      status: status('codex', true, false, {
        effectiveKind: 'api',
        effectiveLabel: 'AgentHub Codex 本机路由',
      }),
    });
    expect(view.currentLoginTitle).toBeNull();
    expect(view.label).toBe('未配置连接');
    expect(view.label).not.toContain('本机路由');
  });

  it('shows signed-in for Kiro live oauth/cli auth instead of 未配置', () => {
    for (const health of ['renewable', 'verified'] as const) {
      const kiro = status('kiro', true, false, {
        effectiveKind: 'none',
        effectiveLabel: '未配置',
        authHealth: health,
        authStatus: 'valid',
        authSource: health === 'verified' ? 'kiro-cli whoami' : 'data.sqlite3',
      });
      expect(chatConnectionKind(kiro, false)).toBe('account');
      const view = chatConnectionPickerView(t, {
        primaryAgent: 'kiro',
        status: kiro,
      });
      expect(view.kind).toBe('account');
      expect(view.label).toBe('已登录');
      expect(view.label).not.toContain('未配置');
      expect(view.emptyHint).toBeNull();
    }
  });

  it('does not show pool placeholder 未配置 as the API chip title', () => {
    const view = chatConnectionPickerView(t, {
      primaryAgent: 'kiro',
      status: status('kiro', true, false, {
        effectiveKind: 'none',
        effectiveLabel: '未配置',
        authHealth: 'configured',
        authStatus: 'valid',
        authSource: 'env:KIRO_API_KEY',
      }),
    });
    expect(view.kind).toBe('api');
    expect(view.label).toBe('API');
    expect(view.label).not.toContain('未配置');
  });

  it('hides the unimported-current row until the wallet has loaded', () => {
    const view = chatConnectionPickerView(t, {
      primaryAgent: 'grok',
      walletReady: false,
      status: status('grok', true, false, {
        effectiveKind: 'account',
        effectiveLabel: 'user@example.com',
      }),
    });
    expect(view.label).toBe('user@example.com');
    expect(view.currentLoginTitle).toBeNull();
  });
});

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

function ticket(partial: Partial<TicketView> & Pick<TicketView, 'id'>): TicketView {
  const sourceKind = partial.sourceKind ?? (partial.id.startsWith('provider:') ? 'provider' : 'account');
  const sourceId = partial.sourceId ?? partial.id.slice(partial.id.indexOf(':') + 1);
  return {
    sourceKind,
    sourceId,
    agentId: 'codex',
    label: partial.label ?? partial.id,
    surface: sourceKind === 'account' ? 'codex-chatgpt-subscription' : 'openai-api',
    credentialClass: sourceKind === 'account' ? 'oauth' : 'api_key',
    speaks: [],
    importedFrom: 'codex',
    ...partial,
  };
}

function binding(
  partial: Partial<BindingView> & Pick<BindingView, 'ticketId' | 'agentId'>,
): BindingView {
  return {
    route: 'native',
    active: false,
    profileId: null,
    bridge: null,
    ...partial,
  };
}

function wallet(tickets: TicketView[], bindings: BindingView[] = []): TicketWallet {
  return { tickets, bindings, surfaceGroups: [] };
}

describe('isLeftoverLocalRouteProvider', () => {
  it('does not treat a generic loopback API as leftover 本机路由', () => {
    expect(
      isLeftoverLocalRouteProvider(
        providerRow({
          id: 'litellm-local',
          name: 'LiteLLM',
          configText: 'base_url = "http://127.0.0.1:4000/v1"',
        }),
      ),
    ).toBe(false);
  });

  it('detects generated leftover 本机路由 rows', () => {
    const leftover = providerRow({
      id: 'agenthub_grok_bridge',
      name: 'AgentHub Grok 本机路由',
      configText: 'base_url = "http://127.0.0.1:32123/v1"',
      isCurrent: true,
    });
    expect(isLeftoverLocalRouteProvider(leftover)).toBe(true);
    expect(leftoverProviderIsCurrent([leftover])).toBe(true);
    expect(leftoverProviderIsCurrent([{ ...leftover, isCurrent: false }])).toBe(false);
  });
});

describe('chatConnectionSwitchAction', () => {
  it('switches a native account or provider and binds a login born on another Agent', () => {
    expect(
      chatConnectionSwitchAction(ticket({ id: 'account:codex-1', agentId: 'codex' }), 'codex'),
    ).toEqual({ type: 'switch-account', accountId: 'codex-1' });
    expect(
      chatConnectionSwitchAction(
        ticket({ id: 'provider:openai-1', sourceKind: 'provider', agentId: 'codex' }),
        'codex',
      ),
    ).toEqual({ type: 'switch-provider', providerId: 'openai-1' });
    expect(
      chatConnectionSwitchAction(
        ticket({
          id: 'provider:kimi-1',
          sourceKind: 'provider',
          agentId: 'kimi',
          importedFrom: 'kimi',
        }),
        'codex',
      ),
    ).toEqual({ type: 'bind', ticketId: 'provider:kimi-1' });
  });
});

describe('chatConnectionOptions', () => {
  it('lists official oauth, API Key accounts, and API providers for this Agent', () => {
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet([
        ticket({
          id: 'account:codex-live-1',
          label: 'user@openai.com',
          credentialClass: 'oauth',
        }),
        ticket({
          id: 'account:codex-key-1',
          sourceKind: 'account',
          sourceId: 'codex-key-1',
          label: 'sk-codex',
          surface: 'openai-api',
          credentialClass: 'api_key',
        }),
        ticket({
          id: 'provider:openai-1',
          sourceKind: 'provider',
          label: 'OpenAI',
        }),
      ]),
    });
    expect(options.map((row) => row.ticketId)).toEqual([
      'account:codex-live-1',
      'account:codex-key-1',
      'provider:openai-1',
    ]);
    expect(options[0]).toMatchObject({
      title: 'user@openai.com',
      subtitle: '官方登录',
      action: { type: 'switch-account', accountId: 'codex-live-1' },
    });
    expect(options[1]).toMatchObject({
      title: 'sk-codex',
      subtitle: 'API Key',
      action: { type: 'switch-account', accountId: 'codex-key-1' },
    });
    expect(options[2]).toMatchObject({
      title: 'OpenAI',
      subtitle: 'API Key',
      action: { type: 'switch-provider', providerId: 'openai-1' },
    });
  });

  it('keeps two official logins with the same email instead of collapsing them', () => {
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet([
        ticket({ id: 'account:codex-live-1', label: '41375197@qq.com' }),
        ticket({ id: 'account:codex-live-2', label: '41375197@qq.com' }),
      ]),
    });
    expect(options).toHaveLength(2);
    expect(options.map((row) => row.ticketId)).toEqual([
      'account:codex-live-1',
      'account:codex-live-2',
    ]);
  });

  it('includes a login bound to this Agent even when it was born on another Agent', () => {
    const kimi = ticket({
      id: 'provider:kimi-1',
      sourceKind: 'provider',
      agentId: 'kimi',
      label: 'Kimi 会员',
      surface: 'kimi-code-membership',
      importedFrom: 'kimi',
    });
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet(
        [kimi, ticket({ id: 'account:codex-live-1', label: 'user@openai.com' })],
        [binding({ ticketId: kimi.id, agentId: 'codex', route: 'bridge', active: true })],
      ),
    });
    expect(options.map((row) => row.ticketId)).toEqual([
      'provider:kimi-1',
      'account:codex-live-1',
    ]);
    expect(options[0]).toMatchObject({
      title: 'Kimi 会员',
      subtitle: '本机路由',
      isCurrent: true,
      action: { type: 'bind', ticketId: 'provider:kimi-1' },
    });
    expect(options[1]).toMatchObject({
      title: 'user@openai.com',
      isCurrent: false,
      action: { type: 'switch-account', accountId: 'codex-live-1' },
    });
  });

  it('does not list an unbound login that belongs to another Agent', () => {
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet([
        ticket({
          id: 'provider:kimi-1',
          sourceKind: 'provider',
          agentId: 'kimi',
          label: 'Kimi 会员',
          importedFrom: 'kimi',
        }),
        ticket({ id: 'account:codex-live-1', label: 'user@openai.com' }),
      ]),
    });
    expect(options.map((row) => row.ticketId)).toEqual(['account:codex-live-1']);
  });

  it('does not invent a leftover 本机路由 login', () => {
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet([
        ticket({ id: 'account:codex-live-1', label: 'user@openai.com' }),
      ]),
    });
    expect(options).toHaveLength(1);
    expect(options[0].title).not.toContain('本机路由');
    expect(options.some((row) => row.title === '本机路由')).toBe(false);
  });

  it('puts the checkmark on the active binding ticket, not a leftover provider name', () => {
    const options = chatConnectionOptions(t, {
      agentId: 'codex',
      wallet: wallet(
        [ticket({ id: 'account:codex-live-1', label: '41375197@qq.com' })],
        [
          binding({
            ticketId: 'account:codex-live-1',
            agentId: 'codex',
            route: 'native',
            active: true,
          }),
        ],
      ),
    });
    expect(options[0]).toMatchObject({
      title: '41375197@qq.com',
      subtitle: '官方登录',
      isCurrent: true,
    });
    const chip = chatConnectionPickerView(t, {
      primaryAgent: 'codex',
      status: status('codex', true, false, {
        effectiveKind: 'api',
        effectiveLabel: 'AgentHub Codex 本机路由',
      }),
      currentProviderName: 'agenthub_codex_bridge',
      activeLogin: { title: options[0].title, subtitle: options[0].subtitle },
    });
    expect(chip.label).toBe('41375197@qq.com');
    expect(chip.label).not.toContain('本机路由');
    expect(chip.currentLoginTitle).toBeNull();
  });

  it('returns no options without a wallet or Agent', () => {
    expect(chatConnectionOptions(t, { wallet: null, agentId: 'codex' })).toEqual([]);
    expect(chatConnectionOptions(t, { wallet: wallet([]), agentId: null })).toEqual([]);
  });
});

describe('chatShowsUnimportedCurrent', () => {
  it('shows the live login only when no wallet row is current', () => {
    expect(chatShowsUnimportedCurrent([], 'user@example.com')).toBe(true);
    expect(
      chatShowsUnimportedCurrent(
        [{ isCurrent: true }],
        'user@example.com',
      ),
    ).toBe(false);
    expect(chatShowsUnimportedCurrent([], null)).toBe(false);
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
  it('shares one main-column width for transcript and composer', () => {
    expect(chatMainColumnClass).toBe(pageRhythm.readingColumn);
    expect(chatMainColumnClass).toBe('mx-auto w-full max-w-3xl');
  });

  it('uses a 16px outer stage so transcript and composer share the same inset', () => {
    expect(chatStageClass).toContain('py-4');
  });

  it('keeps the transcript surface transparent so the chat column canvas shows through', () => {
    expect(chatTranscriptSurfaceClass).toBe('bg-transparent');
  });

  it('does not paint bg-panel on the transcript surface', () => {
    expect(chatTranscriptSurfaceClass).not.toContain('bg-panel');
  });
});
