/** Shared chat action definitions for `/` command search. */

export type ChatActionKind = 'local' | 'draft' | 'native';

export type ChatActionId = string;

export interface ChatActionDef {
  id: ChatActionId;
  kind: ChatActionKind;
  /** i18n key under chat.actions.* */
  labelKey?: string;
  /** Literal label for dynamic command items such as model / skill entries. */
  label?: string;
  description?: string;
  /** Optional draft text for sample tasks. */
  draftText?: string;
  keywords: string[];
}

/** Page chrome already has these; `/` only lists in-conversation actions. */
export const CHAT_SLASH_PAGE_NAV_IDS = [
  'open-history',
  'focus-history-search',
  'open-settings',
  'open-agents',
  'open-connections',
] as const;

export const CHAT_ACTIONS: ChatActionDef[] = [
  {
    id: 'new-session',
    kind: 'local',
    labelKey: 'newSession',
    keywords: ['new', '新建', '会话', 'new session', '新聊天', '新对话', '新建对话'],
  },
  {
    id: 'copy-latest-reply',
    kind: 'local',
    labelKey: 'copyLatestReply',
    keywords: ['copy', '复制', '回复', '最近回复', 'clipboard'],
  },
  {
    id: 'sample-understand-project',
    kind: 'draft',
    labelKey: 'sampleUnderstandProject',
    draftText: '请帮我了解这个项目的结构和主要功能。',
    keywords: ['了解', '项目', 'understand', 'project', '结构'],
  },
  {
    id: 'sample-check-issues',
    kind: 'draft',
    labelKey: 'sampleCheckIssues',
    draftText: '请检查这个项目里有没有明显的问题或风险。',
    keywords: ['检查', '问题', 'check', 'issues', '风险'],
  },
  {
    id: 'sample-summarize',
    kind: 'draft',
    labelKey: 'sampleSummarize',
    draftText: '请用简短几句话总结当前目录在做什么。',
    keywords: ['总结', 'summary', 'summarize', '概要'],
  },
  {
    id: 'sample-write-tests',
    kind: 'draft',
    labelKey: 'sampleWriteTests',
    draftText: '请为当前改动补一组最小、可落地的测试，并说明怎么跑。',
    keywords: ['测试', 'test', 'vitest', 'cargo test', '单测'],
  },
  {
    id: 'sample-explain-error',
    kind: 'draft',
    labelKey: 'sampleExplainError',
    draftText: '请根据上面的报错，用通俗说法解释原因，并给出下一步排查。',
    keywords: ['报错', '错误', 'error', 'explain', '排查'],
  },
  {
    id: 'sample-refactor-safe',
    kind: 'draft',
    labelKey: 'sampleRefactorSafe',
    draftText: '请在不改变外部行为的前提下，指出可以安全整理的地方，并给出小步改法。',
    keywords: ['重构', 'refactor', '整理', '可读性'],
  },
  {
    id: 'sample-git-status',
    kind: 'draft',
    labelKey: 'sampleGitStatus',
    draftText: '请查看当前 git 状态，总结未提交改动，并建议合适的提交说明。',
    keywords: ['git', '提交', 'commit', 'status', 'diff'],
  },
];

/** Empty-transcript starter cards. Click fills the composer; it does not send. */
export const CHAT_STARTER_ACTION_IDS = [
  'sample-understand-project',
  'sample-check-issues',
  'sample-summarize',
  'sample-write-tests',
] as const;

export function chatStarterActions(): ChatActionDef[] {
  return CHAT_STARTER_ACTION_IDS.map((id) => {
    const action = CHAT_ACTIONS.find((item) => item.id === id);
    if (!action) throw new Error(`missing chat starter action: ${id}`);
    return action;
  });
}

export type ChatStarterCopyKey = 'understand' | 'check' | 'summarize' | 'tests';

export function chatStarterCopyKey(id: string): ChatStarterCopyKey | null {
  switch (id) {
    case 'sample-understand-project':
      return 'understand';
    case 'sample-check-issues':
      return 'check';
    case 'sample-summarize':
      return 'summarize';
    case 'sample-write-tests':
      return 'tests';
    default:
      return null;
  }
}

export type ChatActionDisableReason = 'noReply' | 'noAgent';

export interface ChatActionContext {
  hasLatestReply: boolean;
  newChatAllowed: boolean;
}

/** True only for explicit command-search mode: empty draft or a single leading `/…` or `\\…` token. */
export function isCommandSearchMode(draft: string): boolean {
  const value = draft;
  if (value === '/' || value === '\\') return true;
  const lead = value[0];
  if (lead !== '/' && lead !== '\\') return false;
  // Mid-prose, paths, and code with `/` elsewhere must not open the menu.
  if (/\s/.test(value)) return false;
  if (value.includes('://')) return false;
  if (value.length > 1 && (value[1] === '/' || value[1] === '\\')) return false;
  return lead === '/' ? /^\/[^\/\s]*$/.test(value) : /^\\[^\\\s]*$/.test(value);
}

export function normalizeActionQuery(raw: string): string {
  return raw.normalize('NFKC').trim().toLowerCase();
}

export function commandSearchQuery(draft: string): string {
  if (!isCommandSearchMode(draft)) return '';
  return normalizeActionQuery(draft.slice(1));
}

export function actionMatchesQuery(action: ChatActionDef, query: string): boolean {
  if (!query) return true;
  const hay = normalizeActionQuery(
    [
      action.id,
      action.labelKey ?? '',
      action.label ?? '',
      action.description ?? '',
      action.draftText ?? '',
      ...action.keywords,
    ].join('\u0000'),
  );
  const tokens = query.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return true;
  return tokens.every((token) => hay.includes(token));
}

/** Insert `/name ` so the user can add args; do not send. */
export function nativeSlashDraft(name: string): string {
  const command = name.trim().replace(/^\/+/, '');
  if (!command) return '/';
  return `/${command} `;
}

export function nativeCommandActions(
  commands: ReadonlyArray<{ name: string; description?: string; hint?: string | null }>,
): ChatActionDef[] {
  const seen = new Set<string>();
  const actions: ChatActionDef[] = [];
  for (const command of commands) {
    const name = command.name.trim().replace(/^\/+/, '');
    if (!name || seen.has(name.toLowerCase())) continue;
    seen.add(name.toLowerCase());
    const hint = command.hint?.trim() || undefined;
    const description = [command.description?.trim(), hint].filter(Boolean).join(' · ') || undefined;
    actions.push({
      id: `native-command:${name}`,
      kind: 'native',
      label: `/${name}`,
      description,
      draftText: nativeSlashDraft(name),
      keywords: [name, command.description ?? '', hint ?? '', 'slash', '命令'],
    });
  }
  return actions;
}

/** Slash `/` lists real commands. Sample drafts stay on empty-state chips. */
export function chatOverflowMenuActions(actions: readonly ChatActionDef[] = CHAT_ACTIONS): ChatActionDef[] {
  return actions.filter((item) => item.kind !== 'draft');
}

export function filterChatActions(draft: string, extraActions: ChatActionDef[] = []): ChatActionDef[] {
  if (!isCommandSearchMode(draft)) return [];
  const actions = [...extraActions, ...CHAT_ACTIONS];
  const query = commandSearchQuery(draft);
  const scoped = query ? actions : actions.filter((item) => item.kind !== 'draft');
  if (!query) return scoped;
  return scoped.filter((action) => actionMatchesQuery(action, query));
}

export function chatActionDisabledReason(
  action: ChatActionDef,
  ctx: ChatActionContext,
): ChatActionDisableReason | null {
  if (action.id === 'copy-latest-reply' && !ctx.hasLatestReply) return 'noReply';
  if (action.id === 'new-session' && !ctx.newChatAllowed) return 'noAgent';
  return null;
}

export function clampActionIndex(index: number, length: number): number {
  if (length <= 0) return 0;
  if (index < 0) return length - 1;
  if (index >= length) return 0;
  return index;
}

/** Sit the slash palette just above the textarea (caret lives on line 1; `/` forbids whitespace). */
export function slashMenuFixedPosition(input: {
  anchorTop: number;
  anchorLeft: number;
  viewportHeight: number;
  gap?: number;
}): { left: number; bottom: number } {
  const gap = input.gap ?? 4;
  return {
    left: input.anchorLeft,
    bottom: input.viewportHeight - input.anchorTop + gap,
  };
}
