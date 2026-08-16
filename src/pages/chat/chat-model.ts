/**
 * Chat 页纯函数：会话分组 / 发送前置 / 展示文案。
 * 不 import React、不碰 lib/api。
 */
import { agentDisplayName } from '@/config/agents';
import { processPhaseLabel, type AgentProcessView } from '@/lib/chat-process';
import type { AgentId, AgentStatus, ChatMessage, ChatMessageStatus, Conversation } from '@/lib/types';
import type { TurnGroup } from './chat-format';

export type ChatSendBlocker =
  | { kind: 'hiddenAgents'; agentIds: AgentId[] }
  | { kind: 'unconfiguredAuth'; agentIds: AgentId[] }
  | { kind: 'noCwd' }
  | { kind: 'sendingElsewhere'; conversationId: string; title: string };

export type ChatAgentPickerReason = 'hidden' | 'noAuth';

export type ChatAgentPickerRow = {
  id: AgentId;
  selectable: boolean;
  reason: ChatAgentPickerReason | null;
};

/** 已绑定登录 / API Key 才算配置了授权；未配置或未登录不可选。 */
export function agentHasConfiguredAuth(status: AgentStatus | undefined): boolean {
  if (!status?.installed) return false;
  if (status.effectiveKind && status.effectiveKind !== 'none') return true;
  if (status.authHealth === 'verified' || status.authHealth === 'renewable' || status.authHealth === 'configured') {
    return true;
  }
  if (status.authHealth === 'missing' || status.authHealth === 'needs_login') return false;
  if (status.authLabel === 'API') return true;
  if (status.authStatus === 'valid' || status.authStatus === 'expiring') return true;
  return false;
}

export function isChatAgentSelectable(status: AgentStatus | undefined): boolean {
  return Boolean(status?.installed && !status.hidden && agentHasConfiguredAuth(status));
}

/**
 * 已安装（或已在会话里）的 Agent：可选的在前，已隐藏 / 未配置授权的置底且不可选。
 */
export function chatAgentPickerRows(input: {
  catalogIds: AgentId[];
  agentStatus: AgentStatus[];
  selectedIds: AgentId[];
}): ChatAgentPickerRow[] {
  const byId = new Map(input.agentStatus.map((a) => [a.agentId, a]));
  const rows: ChatAgentPickerRow[] = [];
  for (const id of input.catalogIds) {
    const status = byId.get(id);
    const installed = status?.installed === true;
    const selected = input.selectedIds.includes(id);
    if (!installed && !selected) continue;
    const hidden = Boolean(status?.hidden);
    const noAuth = !agentHasConfiguredAuth(status);
    const selectable = Boolean(installed && !hidden && !noAuth);
    const reason: ChatAgentPickerReason | null = hidden ? 'hidden' : noAuth ? 'noAuth' : null;
    rows.push({ id, selectable, reason });
  }
  return [...rows.filter((r) => r.selectable), ...rows.filter((r) => !r.selectable)];
}

export type ConversationDayKey = 'today' | 'yesterday' | 'week' | 'earlier';

export type ConversationDayGroup = {
  key: ConversationDayKey;
  label: string;
  items: Conversation[];
};

const DAY_LABEL: Record<ConversationDayKey, string> = {
  today: '今天',
  yesterday: '昨天',
  week: '近 7 天',
  earlier: '更早',
};

const RETRY_STATUSES = new Set<ChatMessageStatus>(['failed', 'cancelled', 'timeout']);

export function cwdShortName(cwd: string | null | undefined): string {
  if (cwd == null) return '未设目录';
  const trimmed = cwd.trim();
  if (!trimmed) return '未设目录';
  const stripped = trimmed.replace(/[\\/]+$/, '');
  if (!stripped) {
    // POSIX 根 `/`（或 `///`）去尾分隔后为空，仍应显示 `/`
    return trimmed.includes('/') ? '/' : '未设目录';
  }
  const parts = stripped.split(/[\\/]/);
  return parts[parts.length - 1] || '未设目录';
}

export function filterConversations(convs: Conversation[], query: string): Conversation[] {
  const q = query.trim().toLowerCase();
  if (!q) return convs;
  return convs.filter((c) => {
    const title = (c.title ?? '').toLowerCase();
    const cwd = (c.cwd ?? '').toLowerCase();
    return title.includes(q) || cwd.includes(q);
  });
}

function startOfLocalDay(ms: number): Date {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d;
}

function parseUpdatedAt(iso: string): number {
  const t = Date.parse(iso.includes('T') ? iso : `${iso.replace(' ', 'T')}Z`);
  return Number.isNaN(t) ? 0 : t;
}

export function groupConversationsByDay(
  convs: Conversation[],
  nowMs: number,
): ConversationDayGroup[] {
  const today = startOfLocalDay(nowMs);
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const week = new Date(today);
  week.setDate(week.getDate() - 6);

  const todayStart = today.getTime();
  const yesterdayStart = yesterday.getTime();
  const weekStart = week.getTime();

  const buckets: Record<ConversationDayKey, Conversation[]> = {
    today: [],
    yesterday: [],
    week: [],
    earlier: [],
  };

  for (const c of convs) {
    const t = parseUpdatedAt(c.updatedAt);
    if (t >= todayStart) buckets.today.push(c);
    else if (t >= yesterdayStart) buckets.yesterday.push(c);
    else if (t >= weekStart) buckets.week.push(c);
    else buckets.earlier.push(c);
  }

  const order: ConversationDayKey[] = ['today', 'yesterday', 'week', 'earlier'];
  return order
    .filter((key) => buckets[key].length > 0)
    .map((key) => ({ key, label: DAY_LABEL[key], items: buckets[key] }));
}

export function sendBlockers(input: {
  conversation: Conversation;
  hiddenIds: Set<AgentId>;
  unconfiguredAuthIds?: Set<AgentId>;
  sendingConversationId: string | null;
  sendingTitle?: string;
}): ChatSendBlocker[] {
  const out: ChatSendBlocker[] = [];
  const hidden = input.conversation.agentIds.filter((id) => input.hiddenIds.has(id));
  if (hidden.length > 0) {
    out.push({ kind: 'hiddenAgents', agentIds: hidden });
  }
  const unconfigured = input.conversation.agentIds.filter(
    (id) => !input.hiddenIds.has(id) && input.unconfiguredAuthIds?.has(id),
  );
  if (unconfigured.length > 0) {
    out.push({ kind: 'unconfiguredAuth', agentIds: unconfigured });
  }
  if (!input.conversation.cwd) {
    out.push({ kind: 'noCwd' });
  }
  if (input.sendingConversationId && input.sendingConversationId !== input.conversation.id) {
    out.push({
      kind: 'sendingElsewhere',
      conversationId: input.sendingConversationId,
      title: input.sendingTitle ?? '',
    });
  }
  return out;
}

/** 多选勾选：保持当前顺序；新增追加到末尾。只剩一个时返回 null。 */
export function nextConversationAgentIds(
  current: AgentId[],
  toggleId: AgentId,
): AgentId[] | null {
  if (current.includes(toggleId)) {
    if (current.length === 1) return null;
    return current.filter((id) => id !== toggleId);
  }
  return [...current, toggleId];
}

export function newConversationDefaults(
  active: Conversation | null,
  agentStatus: AgentStatus[],
): { agentIds: AgentId[]; cwd: string | null } {
  const hidden = new Set(agentStatus.filter((a) => a.hidden).map((a) => a.agentId));
  const uninstalled = new Set(
    agentStatus.filter((a) => a.installed === false).map((a) => a.agentId),
  );
  const fallback = agentStatus.find((a) => isChatAgentSelectable(a))?.agentId;
  const fallbackIds: AgentId[] = fallback ? [fallback] : [];

  if (!active) {
    return { agentIds: fallbackIds, cwd: null };
  }

  // 继承当前会话顺序（agentIds[0] 是 primary），只保留可选 Agent
  const byId = new Map(agentStatus.map((a) => [a.agentId, a]));
  const kept = active.agentIds.filter((id) => {
    if (hidden.has(id) || uninstalled.has(id)) return false;
    return agentHasConfiguredAuth(byId.get(id));
  });

  return {
    agentIds: kept.length > 0 ? kept : fallbackIds,
    cwd: active.cwd ?? null,
  };
}

export function agentPickerLabel(active: Conversation | null): string {
  if (!active) return '选择 Agent';
  if (active.agentIds.length === 1) return agentDisplayName(active.agentIds[0]);
  return `${active.agentIds.length} 个 Agent`;
}

export function connectionPickerCaption(opts: {
  agentIds: AgentId[];
  primaryAgent?: AgentId | null;
}): string | null {
  if (opts.agentIds.length <= 1) return null;
  const id = opts.primaryAgent ?? opts.agentIds[0];
  if (!id) return null;
  return `仅作用于首位 Agent（${agentDisplayName(id)}）`;
}

export type ChatConnectionPickerKind = 'account' | 'api' | 'none';

export type ChatConnectionPickerView = {
  kind: ChatConnectionPickerKind;
  label: string;
  subtitle: string | null;
  currentLoginTitle: string | null;
  currentLoginSubtitle: string | null;
  emptyHint: string | null;
  manageLabel: string;
};

export function chatConnectionKind(
  status: AgentStatus | undefined,
  hasCurrentProvider: boolean,
): ChatConnectionPickerKind {
  if (status?.effectiveKind === 'account') return 'account';
  if (status?.effectiveKind === 'api') return 'api';
  if (agentHasConfiguredAuth(status)) {
    if (status?.authLabel === 'API' || status?.authHealth === 'configured') return 'api';
    return 'account';
  }
  if (hasCurrentProvider) return 'api';
  return 'none';
}

function accountConnectionTitle(status: AgentStatus | undefined): string {
  const label = status?.effectiveLabel?.trim() || status?.currentProvider?.trim();
  if (label && label !== '未配置') return label;
  return '已登录';
}

export function chatConnectionPickerView(input: {
  primaryAgent: AgentId | null;
  switching?: boolean;
  status?: AgentStatus;
  currentProviderName?: string | null;
  currentProviderModel?: string | null;
}): ChatConnectionPickerView {
  if (!input.primaryAgent) {
    return {
      kind: 'none',
      label: '切换连接',
      subtitle: null,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: '去 Connections 添加',
    };
  }

  const kind = chatConnectionKind(input.status, Boolean(input.currentProviderName));
  if (input.switching) {
    return {
      kind,
      label: '切换中…',
      subtitle: null,
      currentLoginTitle: kind === 'account' ? accountConnectionTitle(input.status) : null,
      currentLoginSubtitle: kind === 'account' ? '当前登录' : null,
      emptyHint: kind === 'none' ? '暂无连接' : null,
      manageLabel: kind === 'none' ? '去 Connections 添加' : '去 Connections 管理',
    };
  }

  if (kind === 'account') {
    const title = accountConnectionTitle(input.status);
    return {
      kind,
      label: title,
      subtitle: null,
      currentLoginTitle: title,
      currentLoginSubtitle: '当前登录',
      emptyHint: null,
      manageLabel: '去 Connections 管理',
    };
  }

  if (kind === 'api') {
    const title = input.currentProviderName?.trim() || input.status?.effectiveLabel?.trim() || 'API';
    return {
      kind,
      label: title,
      subtitle: input.currentProviderModel?.trim() || null,
      currentLoginTitle: input.currentProviderName ? null : title,
      currentLoginSubtitle: input.currentProviderName ? null : 'API',
      emptyHint: null,
      manageLabel: '去 Connections 管理',
    };
  }

  return {
    kind: 'none',
    label: '未配置连接',
    subtitle: null,
    currentLoginTitle: null,
    currentLoginSubtitle: null,
    emptyHint: '暂无连接',
    manageLabel: '去 Connections 添加',
  };
}

export function messageStatusLabel(
  status: string,
  process?: AgentProcessView,
): string | null {
  // 过程机更细（排队/启动）；终态以 message.status 为准
  if (process && (status === 'running' || !status)) {
    if (process.phase === 'queued' || process.phase === 'starting' || process.phase === 'running') {
      return processPhaseLabel(process.phase);
    }
  }
  switch (status) {
    case 'running':
      return '生成中';
    case 'error':
    case 'failed':
      return '失败';
    case 'cancelled':
      return '已取消';
    case 'timeout':
      return '超时';
    case 'ok':
    case 'done':
    case 'success':
      return null;
    default:
      return status;
  }
}

export function visibleAgentDots(agentIds: AgentId[]): { shown: AgentId[]; extra: number } {
  const shown = agentIds.slice(0, 3);
  return { shown, extra: Math.max(0, agentIds.length - 3) };
}

export function retryTarget(
  turns: TurnGroup[],
  sending: boolean,
): { turn: number; prompt: string } | null {
  if (sending || turns.length === 0) return null;
  const last = turns[turns.length - 1];
  const prompt = last.user?.content?.trim() ?? '';
  if (!prompt) return null;
  const retryable = last.agents.some((m) => RETRY_STATUSES.has(m.status));
  if (!retryable) return null;
  return { turn: last.turn, prompt };
}

export function turnComparisonChips(agents: ChatMessage[]): Array<{
  agentId: AgentId;
  status: ChatMessageStatus;
  durationMs: number;
  messageId: string;
}> {
  return agents
    .filter((m): m is ChatMessage & { agentId: AgentId } => Boolean(m.agentId))
    .map((m) => ({
      agentId: m.agentId,
      status: m.status,
      durationMs: m.durationMs,
      messageId: m.id,
    }));
}

export function conversationTitle(title: string): string {
  return title.trim() ? title : '新对话';
}

export type ChatBlockerPrimaryTarget =
  | 'agents'
  | 'connections'
  | 'pick-directory'
  | 'settings';

export function blockerPrimaryTarget(
  blocker: Pick<ChatSendBlocker, 'kind'>,
): ChatBlockerPrimaryTarget {
  switch (blocker.kind) {
    case 'hiddenAgents':
      return 'agents';
    case 'unconfiguredAuth':
      return 'connections';
    case 'noCwd':
      return 'pick-directory';
    case 'sendingElsewhere':
      return 'settings';
  }
}

export function blockerCopy(blocker: ChatSendBlocker): {
  text: string;
  primaryAction: string;
  secondaryAction?: string;
} {
  switch (blocker.kind) {
    case 'hiddenAgents':
      return {
        text: '会话包含已隐藏 Agent，暂不能发送',
        primaryAction: '去 Agents 页',
      };
    case 'unconfiguredAuth':
      return {
        text: '会话包含未配置授权的 Agent，暂不能发送',
        primaryAction: '去 Connections 页',
      };
    case 'noCwd':
      return {
        text: '未设置工作目录 — Agent 需要在指定目录内工作',
        primaryAction: '设置工作目录',
      };
    case 'sendingElsewhere':
      return {
        text: `「${conversationTitle(blocker.title)}」正在生成`,
        primaryAction: '回到该会话',
        secondaryAction: '停止',
      };
  }
}
