/** Shared chat action definitions for the menu button and `/` command search. */

export type ChatActionKind = 'local' | 'draft';

export type ChatActionId =
  | 'new-session'
  | 'open-history'
  | 'copy-latest-reply'
  | 'open-settings'
  | 'sample-understand-project'
  | 'sample-check-issues'
  | 'sample-summarize';

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
    keywords: ['new', '新建', '会话', 'new session'],
  },
  {
    id: 'open-history',
    kind: 'local',
    labelKey: 'openHistory',
    keywords: ['history', '历史', '记录'],
  },
  {
    id: 'copy-latest-reply',
    kind: 'local',
    labelKey: 'copyLatestReply',
    keywords: ['copy', '复制', '回复'],
  },
  {
    id: 'open-settings',
    kind: 'local',
    labelKey: 'openSettings',
    keywords: ['settings', '设置', '偏好'],
  },
  {
    id: 'sample-understand-project',
    kind: 'draft',
    labelKey: 'sampleUnderstandProject',
    draftText: '请帮我了解这个项目的结构和主要功能。',
    keywords: ['了解', '项目', 'understand', 'project'],
  },
  {
    id: 'sample-check-issues',
    kind: 'draft',
    labelKey: 'sampleCheckIssues',
    draftText: '请检查这个项目里有没有明显的问题或风险。',
    keywords: ['检查', '问题', 'check', 'issues'],
  },
  {
    id: 'sample-summarize',
    kind: 'draft',
    labelKey: 'sampleSummarize',
    draftText: '请用简短几句话总结当前目录在做什么。',
    keywords: ['总结', 'summary', 'summarize'],
  },
];

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

export function commandSearchQuery(draft: string): string {
  if (!isCommandSearchMode(draft)) return '';
  return draft.slice(1).trim().toLowerCase();
}

export function filterChatActions(draft: string): ChatActionDef[] {
  if (!isCommandSearchMode(draft)) return [];
  const query = commandSearchQuery(draft);
  if (!query) return CHAT_ACTIONS;
  return CHAT_ACTIONS.filter((action) => {
    const hay = [action.id, action.labelKey, ...action.keywords].join(' ').toLowerCase();
    return hay.includes(query);
  });
}
