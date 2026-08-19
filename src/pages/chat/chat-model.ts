/**
 * Chat 页纯函数：会话分组 / 发送前置 / 展示文案。
 * 不 import React、不碰 lib/api。
 */
import { agentDisplayName } from '@/config/agents';
import { interpolate, type MessageKey, type MessageParams, type TranslateFn } from '@/lib/i18n';
import { processPhaseLabel, type AgentProcessView } from '@/lib/chat-process';
import type { AgentId, AgentStatus, ChatMessage, ChatMessageStatus, Conversation } from '@/lib/types';
import type { TurnGroup } from './chat-format';

function tx(t: TranslateFn | undefined, key: MessageKey, fallback: string, params?: MessageParams): string {
  return t ? t(key, params) : interpolate(fallback, params);
}

export type ChatSendBlocker =
  | { kind: 'hiddenAgents'; agentIds: AgentId[] }
  | { kind: 'unconfiguredAuth'; agentIds: AgentId[] }
  | { kind: 'noCwd' }
  | { kind: 'sendingElsewhere'; conversationId: string; title: string };

export type ChatAgentPickerReason = 'noAuth';

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
 * 已安装且未隐藏的 Agent：未配置授权的置底、灰显不可选。隐藏的不进列表。
 */
export function chatAgentPickerRows(input: {
  catalogIds: AgentId[];
  agentStatus: AgentStatus[];
}): ChatAgentPickerRow[] {
  const byId = new Map(input.agentStatus.map((a) => [a.agentId, a]));
  const rows: ChatAgentPickerRow[] = [];
  for (const id of input.catalogIds) {
    const status = byId.get(id);
    if (status?.installed !== true || status.hidden) continue;
    const noAuth = !agentHasConfiguredAuth(status);
    rows.push({
      id,
      selectable: !noAuth,
      reason: noAuth ? 'noAuth' : null,
    });
  }
  return [...rows.filter((r) => r.selectable), ...rows.filter((r) => !r.selectable)];
}

/** 列表为空时的原因：未就绪不得当成「没装」。安装/全隐藏在 picker 里不必拆开。 */
export type ChatPickerEmptyKind = 'loading' | 'none';

export function chatAgentPickerEmptyKind(input: {
  agentsReady: boolean;
  rowCount: number;
}): ChatPickerEmptyKind | null {
  if (input.rowCount > 0) return null;
  return input.agentsReady ? 'none' : 'loading';
}

export function chatAgentPickerEmptyCopy(
  t: TranslateFn | undefined,
  kind: ChatPickerEmptyKind,
): {
  text: string;
  action: string | null;
} {
  if (kind === 'loading') {
    return { text: tx(t, 'chat.picker.loading', '正在检测已安装的 Agent…'), action: null };
  }
  return {
    text: tx(t, 'chat.picker.none', '没有可选择的 Agent'),
    action: tx(t, 'chat.picker.goAgents', '去 Agents 页'),
  };
}

export type ConversationDayKey = 'today' | 'yesterday' | 'week' | 'earlier';

export type ConversationDayGroup = {
  key: ConversationDayKey;
  label: string;
  items: Conversation[];
};

const DAY_KEYS: Record<
  ConversationDayKey,
  { key: 'chat.day.today' | 'chat.day.yesterday' | 'chat.day.week' | 'chat.day.earlier'; fallback: string }
> = {
  today: { key: 'chat.day.today', fallback: '今天' },
  yesterday: { key: 'chat.day.yesterday', fallback: '昨天' },
  week: { key: 'chat.day.week', fallback: '近 7 天' },
  earlier: { key: 'chat.day.earlier', fallback: '更早' },
};

const RETRY_STATUSES = new Set<ChatMessageStatus>(['failed', 'cancelled', 'timeout']);

export function cwdShortName(cwd: string | null | undefined, t?: TranslateFn): string {
  const unset = tx(t, 'chat.cwd.unset', '未设目录');
  if (cwd == null) return unset;
  const trimmed = cwd.trim();
  if (!trimmed) return unset;
  const stripped = trimmed.replace(/[\\/]+$/, '');
  if (!stripped) {
    // POSIX 根 `/`（或 `///`）去尾分隔后为空，仍应显示 `/`
    return trimmed.includes('/') ? '/' : unset;
  }
  const parts = stripped.split(/[\\/]/);
  return parts[parts.length - 1] || unset;
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
  t?: TranslateFn,
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
    const ts = parseUpdatedAt(c.updatedAt);
    if (ts >= todayStart) buckets.today.push(c);
    else if (ts >= yesterdayStart) buckets.yesterday.push(c);
    else if (ts >= weekStart) buckets.week.push(c);
    else buckets.earlier.push(c);
  }

  const order: ConversationDayKey[] = ['today', 'yesterday', 'week', 'earlier'];
  return order
    .filter((key) => buckets[key].length > 0)
    .map((key) => ({
      key,
      label: tx(t, DAY_KEYS[key].key, DAY_KEYS[key].fallback),
      items: buckets[key],
    }));
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

/**
 * Chat headless 自动批准的真实效果，对齐各 adapter `build_run_spec`。
 * 不是 capability 标牌：Kimi/DSH 在 TUI 里有 yolo，但 -p / headless 不会加上。
 */
export type AutoApproveEffect = 'skip' | 'project-trust' | 'none';

export function autoApproveEffect(agentId: AgentId | null | undefined): AutoApproveEffect {
  switch (agentId) {
    case 'claude':
    case 'codex':
    case 'grok':
    case 'workbuddy':
    case 'cursor':
      return 'skip';
    case 'pi':
      return 'project-trust';
    default:
      return 'none';
  }
}

export function autoApproveActive(
  allowDangerous: boolean,
  agentId: AgentId | null | undefined,
): boolean {
  return allowDangerous && autoApproveEffect(agentId) !== 'none';
}

export function autoApproveHint(t: TranslateFn | undefined, effect: AutoApproveEffect): string {
  switch (effect) {
    case 'skip':
      return tx(t, 'chat.autoApprove.skip', '跳过工具确认');
    case 'project-trust':
      return tx(t, 'chat.autoApprove.projectTrust', '仅信任项目文件，不是完全跳过确认');
    case 'none':
      return tx(t, 'chat.autoApprove.none', '此 Agent 的 headless 模式无法跳过确认');
  }
}

export function autoApproveFooter(
  t: TranslateFn | undefined,
  allowDangerous: boolean,
  agentId: AgentId | null | undefined,
): { text: string; warning: boolean } {
  const effect = autoApproveEffect(agentId);
  if (!allowDangerous) {
    return { text: tx(t, 'chat.autoApprove.footerOff', 'Agent 可能修改工作目录中的文件'), warning: false };
  }
  if (effect === 'skip') {
    return {
      text: tx(t, 'chat.autoApprove.footerSkip', '自动批准已开启 · Agent 将不经确认修改文件'),
      warning: true,
    };
  }
  if (effect === 'project-trust') {
    return {
      text: tx(t, 'chat.autoApprove.footerTrust', '自动批准已开启 · 仅信任项目文件，仍可能要求确认'),
      warning: true,
    };
  }
  return {
    text: tx(t, 'chat.autoApprove.footerNone', '此 Agent 无法在 Chat 中跳过确认，自动批准不会生效'),
    warning: false,
  };
}

export function autoApproveConfirmCopy(t: TranslateFn | undefined, effect: AutoApproveEffect): string {
  if (effect === 'project-trust') {
    return tx(
      t,
      'chat.autoApprove.confirmTrust',
      '开启后会信任当前项目内的文件。该 Agent 不会完全跳过工具确认，仍可能停下等待批准。仅在信任当前工作目录时开启。',
    );
  }
  return tx(
    t,
    'chat.autoApprove.confirmSkip',
    '开启后将跳过工具确认，Agent 可直接改文件、执行命令。仅在信任当前工作目录时开启。',
  );
}

/** 单选：点当前项不变；点其他项替换。无法跳过确认的 Agent 会清掉已开的自动批准。 */
export function selectConversationAgent(input: {
  currentIds: AgentId[];
  nextId: AgentId;
  allowDangerous: boolean;
}): { agentIds: AgentId[]; allowDangerous?: boolean } | null {
  if (input.currentIds.length === 1 && input.currentIds[0] === input.nextId) {
    return null;
  }
  const patch: { agentIds: AgentId[]; allowDangerous?: boolean } = {
    agentIds: [input.nextId],
  };
  if (input.allowDangerous && autoApproveEffect(input.nextId) === 'none') {
    patch.allowDangerous = false;
  }
  return patch;
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

  const byId = new Map(agentStatus.map((a) => [a.agentId, a]));
  const kept = active.agentIds.filter((id) => {
    if (hidden.has(id) || uninstalled.has(id)) return false;
    return agentHasConfiguredAuth(byId.get(id));
  });

  return {
    agentIds: kept.length > 0 ? [kept[0]] : fallbackIds,
    cwd: active.cwd ?? null,
  };
}

export function agentPickerLabel(t: TranslateFn | undefined, active: Conversation | null): string {
  const id = active?.agentIds[0];
  return id ? agentDisplayName(id) : tx(t, 'chat.picker.selectAgent', '选择 Agent');
}

export function connectionPickerCaption(
  t: TranslateFn | undefined,
  opts: {
    agentIds: AgentId[];
    primaryAgent?: AgentId | null;
  },
): string | null {
  if (opts.agentIds.length <= 1) return null;
  const id = opts.primaryAgent ?? opts.agentIds[0];
  if (!id) return null;
  const name = agentDisplayName(id);
  return tx(t, 'chat.connection.caption', '仅作用于首位 Agent（{name}）', { name });
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

function accountConnectionTitle(t: TranslateFn | undefined, status: AgentStatus | undefined): string {
  const label = status?.effectiveLabel?.trim() || status?.currentProvider?.trim();
  const unconfigured = tx(t, 'chat.connection.unconfiguredLabel', '未配置');
  if (label && label !== unconfigured) return label;
  return tx(t, 'chat.connection.signedIn', '已登录');
}

export function chatConnectionPickerView(
  t: TranslateFn | undefined,
  input: {
    primaryAgent: AgentId | null;
    switching?: boolean;
    status?: AgentStatus;
    currentProviderName?: string | null;
    currentProviderModel?: string | null;
  },
): ChatConnectionPickerView {
  if (!input.primaryAgent) {
    return {
      kind: 'none',
      label: tx(t, 'chat.connection.switch', '切换连接'),
      subtitle: null,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: tx(t, 'chat.connection.add', '去 Connections 添加'),
    };
  }

  const kind = chatConnectionKind(input.status, Boolean(input.currentProviderName));
  if (input.switching) {
    return {
      kind,
      label: tx(t, 'chat.connection.switching', '切换中…'),
      subtitle: null,
      currentLoginTitle: kind === 'account' ? accountConnectionTitle(t, input.status) : null,
      currentLoginSubtitle: kind === 'account' ? tx(t, 'chat.connection.currentLogin', '当前登录') : null,
      emptyHint: kind === 'none' ? tx(t, 'chat.connection.none', '暂无连接') : null,
      manageLabel:
        kind === 'none'
          ? tx(t, 'chat.connection.add', '去 Connections 添加')
          : tx(t, 'chat.connection.manage', '去 Connections 管理'),
    };
  }

  if (kind === 'account') {
    const title = accountConnectionTitle(t, input.status);
    return {
      kind,
      label: title,
      subtitle: null,
      currentLoginTitle: title,
      currentLoginSubtitle: tx(t, 'chat.connection.currentLogin', '当前登录'),
      emptyHint: null,
      manageLabel: tx(t, 'chat.connection.manage', '去 Connections 管理'),
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
      manageLabel: tx(t, 'chat.connection.manage', '去 Connections 管理'),
    };
  }

  return {
    kind: 'none',
    label: tx(t, 'chat.connection.unconfigured', '未配置连接'),
    subtitle: null,
    currentLoginTitle: null,
    currentLoginSubtitle: null,
    emptyHint: tx(t, 'chat.connection.none', '暂无连接'),
    manageLabel: tx(t, 'chat.connection.add', '去 Connections 添加'),
  };
}

export function messageStatusLabel(
  t: TranslateFn | undefined,
  status: string,
  process?: AgentProcessView,
): string | null {
  // 过程机更细（排队/启动）；终态以 message.status 为准
  if (process && (status === 'running' || !status)) {
    if (process.phase === 'queued' || process.phase === 'starting' || process.phase === 'running') {
      return processPhaseLabel(process.phase, t);
    }
  }
  switch (status) {
    case 'running':
      return tx(t, 'chat.status.generating', '生成中');
    case 'error':
    case 'failed':
      return tx(t, 'chat.status.failed', '失败');
    case 'cancelled':
      return tx(t, 'chat.status.cancelled', '已取消');
    case 'timeout':
      return tx(t, 'chat.status.timeout', '超时');
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

export function conversationTitle(t: TranslateFn | undefined, title: string): string {
  return title.trim() ? title : tx(t, 'chat.title.newConversation', '新对话');
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

export function blockerCopy(
  t: TranslateFn | undefined,
  blocker: ChatSendBlocker,
): {
  text: string;
  primaryAction: string;
  secondaryAction?: string;
} {
  switch (blocker.kind) {
    case 'hiddenAgents':
      return {
        text: tx(t, 'chat.blocker.hidden', '会话包含已隐藏 Agent，暂不能发送'),
        primaryAction: tx(t, 'chat.blocker.goAgents', '去 Agents 页'),
      };
    case 'unconfiguredAuth':
      return {
        text: tx(t, 'chat.blocker.unconfigured', '会话包含未配置授权的 Agent，暂不能发送'),
        primaryAction: tx(t, 'chat.blocker.goConnections', '去 Connections 页'),
      };
    case 'noCwd':
      return {
        text: tx(t, 'chat.blocker.noCwd', '未设置工作目录 — Agent 需要在指定目录内工作'),
        primaryAction: tx(t, 'chat.blocker.setCwd', '设置工作目录'),
      };
    case 'sendingElsewhere': {
      const title = conversationTitle(t, blocker.title);
      return {
        text: tx(t, 'chat.blocker.generating', '「{title}」正在生成', { title }),
        primaryAction: tx(t, 'chat.blocker.backToSession', '回到该会话'),
        secondaryAction: tx(t, 'chat.blocker.stop', '停止'),
      };
    }
  }
}
