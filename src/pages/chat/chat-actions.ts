/** Shared chat action definitions for the menu button and `/` command search. */

export type ChatActionKind = 'local' | 'draft';

export type ChatActionId =
  | 'new-session'
  | 'open-history'
  | 'focus-history-search'
  | 'copy-latest-reply'
  | 'open-settings'
  | 'open-agents'
  | 'open-connections'
  | 'sample-understand-project'
  | 'sample-check-issues'
  | 'sample-summarize'
  | 'sample-write-tests'
  | 'sample-explain-error'
  | 'sample-refactor-safe'
  | 'sample-git-status';

export interface ChatActionDef {
  id: ChatActionId;
  kind: ChatActionKind;
  /** i18n key under chat.actions.* */
  labelKey: string;
  /** Optional draft text for sample tasks. */
  draftText?: string;
  keywords: string[];
}

export const CHAT_ACTIONS: ChatActionDef[] = [
  {
    id: 'new-session',
    kind: 'local',
    labelKey: 'newSession',
    keywords: ['new', '新建', '会话', 'new session', '新聊天', '新对话'],
  },
  {
    id: 'open-history',
    kind: 'local',
    labelKey: 'openHistory',
    keywords: ['history', '历史', '记录', '会话列表', 'rail'],
  },
  {
    id: 'focus-history-search',
    kind: 'local',
    labelKey: 'focusHistorySearch',
    keywords: ['search', '搜索', '查找', '历史搜索', 'find history'],
  },
  {
    id: 'copy-latest-reply',
    kind: 'local',
    labelKey: 'copyLatestReply',
    keywords: ['copy', '复制', '回复', '最近回复', 'clipboard'],
  },
  {
    id: 'open-settings',
    kind: 'local',
    labelKey: 'openSettings',
    keywords: ['settings', '设置', '偏好', '危险模式', '工作目录'],
  },
  {
    id: 'open-agents',
    kind: 'local',
    labelKey: 'openAgents',
    keywords: ['agents', '代理', '智能体', '安装', 'agents page'],
  },
  {
    id: 'open-connections',
    kind: 'local',
    labelKey: 'openConnections',
    keywords: ['connections', '连接', '登录', '账号', 'connections page'],
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

export type ChatActionDisableReason = 'noReply' | 'noAgent';

export interface ChatActionContext {
  hasLatestReply: boolean;
  newChatAllowed: boolean;
}

/** True only for explicit command-search mode: empty draft or a single leading `/…` token. */
export function isCommandSearchMode(draft: string): boolean {
  const value = draft;
  if (value === '/') return true;
  if (!value.startsWith('/')) return false;
  // Mid-prose, paths, and code with `/` elsewhere must not open the menu.
  if (/\s/.test(value)) return false;
  if (value.includes('://')) return false;
  if (value.length > 1 && value[1] === '/') return false; // UNC / absolute-ish
  return /^\/[^\/\s]*$/.test(value);
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
    [action.id, action.labelKey, action.draftText ?? '', ...action.keywords].join('\u0000'),
  );
  const tokens = query.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return true;
  return tokens.every((token) => hay.includes(token));
}

export function filterChatActions(draft: string): ChatActionDef[] {
  if (!isCommandSearchMode(draft)) return [];
  const query = commandSearchQuery(draft);
  if (!query) return CHAT_ACTIONS;
  return CHAT_ACTIONS.filter((action) => actionMatchesQuery(action, query));
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
