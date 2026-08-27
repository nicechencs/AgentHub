/**
 * Chat 页纯函数：会话分组 / 发送前置 / 展示文案。
 * 不 import React、不碰 lib/api。
 */
import { pageRhythm } from '@/components/layout/page-rhythm';
import { agentDisplayName } from '@/config/agents';
import { sliceAgentStatus } from '@/lib/backend/contracts/agent-status-view';
import type {
  BindingRoute,
  TicketView,
  TicketWallet,
} from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';
import { processPhaseLabel, type AgentProcessView } from '@/lib/chat-process';
import { nativeResumeCommand } from '@/lib/session-resume';
import {
  activeBindingForAgent,
  filterTicketsByAgentUsage,
} from '@/lib/ticket-wallet';
import type {
  AgentId,
  AgentStatus,
  ChatMessage,
  ChatMessageStatus,
  Conversation,
} from '@/lib/types';
import type { TurnGroup } from './chat-format';

export type ChatSendBlocker =
  | { kind: 'hiddenAgents'; agentIds: AgentId[] }
  | { kind: 'envNotReady'; agentIds: AgentId[] }
  | { kind: 'unconfiguredAuth'; agentIds: AgentId[] }
  | { kind: 'noCwd' }
  | { kind: 'sendingElsewhere'; conversationId: string; title: string };

export type ChatAgentPickerReason = 'noAuth' | 'envNotReady';

export type ChatAgentPickerRow = {
  id: AgentId;
  selectable: boolean;
  reason: ChatAgentPickerReason | null;
};

/** Pi Chat needs Node 22.19 (`envReady`); other agents' envReady is install-channel only. */
export function agentChatEnvReady(status: AgentStatus | undefined): boolean {
  if (!status || status.agentId !== 'pi') return true;
  return sliceAgentStatus(status).env.ready !== false;
}

/** 已绑定登录 / API Key 才算配置了授权；未配置或未登录不可选。 */
export function agentHasConfiguredAuth(status: AgentStatus | undefined): boolean {
  if (!status?.installed) return false;
  const view = sliceAgentStatus(status);
  if (view.effectiveConnection.kind !== 'unset' && view.effectiveConnection.kind !== 'none') {
    return true;
  }
  if (
    view.liveAuth.health === 'verified'
    || view.liveAuth.health === 'renewable'
    || view.liveAuth.health === 'configured'
  ) {
    return true;
  }
  if (view.liveAuth.health === 'missing' || view.liveAuth.health === 'needs_login') return false;
  return false;
}

export function isChatAgentSelectable(status: AgentStatus | undefined): boolean {
  return Boolean(
    status?.installed
      && sliceAgentStatus(status).hidden !== 'hidden'
      && agentHasConfiguredAuth(status)
      && agentChatEnvReady(status),
  );
}

/**
 * 已安装且未隐藏的 Agent：未配置授权的置底、灰显不可选。隐藏的不进列表。
 */
export function chatAgentPickerRows(input: {
  catalogIds: readonly AgentId[];
  agentStatus: AgentStatus[];
}): ChatAgentPickerRow[] {
  const byId = new Map(input.agentStatus.map((a) => [a.agentId, a]));
  const rows: ChatAgentPickerRow[] = [];
  for (const id of input.catalogIds) {
    const status = byId.get(id);
    if (status?.installed !== true || sliceAgentStatus(status).hidden === 'hidden') continue;
    const envNotReady = !agentChatEnvReady(status);
    const noAuth = !agentHasConfiguredAuth(status);
    const reason: ChatAgentPickerReason | null = envNotReady
      ? 'envNotReady'
      : noAuth
        ? 'noAuth'
        : null;
    rows.push({
      id,
      selectable: reason === null,
      reason,
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

export function chatAgentPickerEmptyCopy(t: TranslateFn, kind: ChatPickerEmptyKind): {
  text: string;
  action: string | null;
} {
  if (kind === 'loading') {
    return { text: t('chat.picker.loading'), action: null };
  }
  return { text: t('chat.picker.none'), action: t('chat.picker.goAgents') };
}

export type ConversationDayKey = 'today' | 'yesterday' | 'week' | 'earlier';

export type ConversationDayGroup = {
  key: ConversationDayKey;
  label: string;
  items: Conversation[];
};

const DAY_KEYS: Record<ConversationDayKey, 'chat.day.today' | 'chat.day.yesterday' | 'chat.day.week' | 'chat.day.earlier'> = {
  today: 'chat.day.today',
  yesterday: 'chat.day.yesterday',
  week: 'chat.day.week',
  earlier: 'chat.day.earlier',
};

const RETRY_STATUSES = new Set<ChatMessageStatus>(['failed', 'cancelled', 'timeout']);

/** Interactive TUI resume command for a Hub conversation, when a native id is known. */
export function conversationResumeCommand(c: Pick<Conversation, 'agentIds' | 'nativeSessionId'>): string | null {
  const agent = c.agentIds[0];
  if (!agent) return null;
  return nativeResumeCommand(agent, c.nativeSessionId);
}

export function cwdShortName(cwd: string | null | undefined, t: TranslateFn): string {
  if (cwd == null) return t('chat.cwd.unset');
  const trimmed = cwd.trim();
  if (!trimmed) return t('chat.cwd.unset');
  const stripped = trimmed.replace(/[\\/]+$/, '');
  if (!stripped) {
    // POSIX 根 `/`（或 `///`）去尾分隔后为空，仍应显示 `/`
    return trimmed.includes('/') ? '/' : t('chat.cwd.unset');
  }
  const parts = stripped.split(/[\\/]/);
  return parts[parts.length - 1] || t('chat.cwd.unset');
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
  t: TranslateFn,
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
    .map((key) => ({ key, label: t(DAY_KEYS[key]), items: buckets[key] }));
}

export function sendBlockers(input: {
  conversation: Conversation;
  hiddenIds: Set<AgentId>;
  envNotReadyIds?: Set<AgentId>;
  unconfiguredAuthIds?: Set<AgentId>;
  sendingConversationId: string | null;
  sendingTitle?: string;
}): ChatSendBlocker[] {
  const out: ChatSendBlocker[] = [];
  const hidden = input.conversation.agentIds.filter((id) => input.hiddenIds.has(id));
  if (hidden.length > 0) {
    out.push({ kind: 'hiddenAgents', agentIds: hidden });
  }
  const envNotReady = input.conversation.agentIds.filter(
    (id) => !input.hiddenIds.has(id) && input.envNotReadyIds?.has(id),
  );
  if (envNotReady.length > 0) {
    out.push({ kind: 'envNotReady', agentIds: envNotReady });
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

export function autoApproveHint(t: TranslateFn, effect: AutoApproveEffect): string {
  switch (effect) {
    case 'skip':
      return t('chat.autoApprove.skip');
    case 'project-trust':
      return t('chat.autoApprove.projectTrust');
    case 'none':
      return t('chat.autoApprove.none');
  }
}

export function autoApproveFooter(
  t: TranslateFn,
  allowDangerous: boolean,
  agentId: AgentId | null | undefined,
): { text: string; warning: boolean } {
  const effect = autoApproveEffect(agentId);
  if (!allowDangerous) {
    return { text: t('chat.autoApprove.footerOff'), warning: false };
  }
  if (effect === 'skip') {
    return { text: t('chat.autoApprove.footerSkip'), warning: true };
  }
  if (effect === 'project-trust') {
    return { text: t('chat.autoApprove.footerTrust'), warning: true };
  }
  return { text: t('chat.autoApprove.footerNone'), warning: false };
}

export function autoApproveConfirmCopy(t: TranslateFn, effect: AutoApproveEffect): string {
  if (effect === 'project-trust') {
    return t('chat.autoApprove.confirmTrust');
  }
  return t('chat.autoApprove.confirmSkip');
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

export function agentPickerLabel(t: TranslateFn, active: Conversation | null): string {
  const id = active?.agentIds[0];
  return id ? agentDisplayName(id) : t('chat.picker.selectAgent');
}

export function connectionPickerCaption(t: TranslateFn, opts: {
  agentIds: AgentId[];
  primaryAgent?: AgentId | null;
}): string | null {
  if (opts.agentIds.length <= 1) return null;
  const id = opts.primaryAgent ?? opts.agentIds[0];
  if (!id) return null;
  return t('chat.connection.caption', { name: agentDisplayName(id) });
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
  const kind = sliceAgentStatus(status ?? {}).effectiveConnection.kind;
  if (kind === 'account') return 'account';
  if (kind === 'api') return 'api';
  if (agentHasConfiguredAuth(status)) {
    const health = sliceAgentStatus(status ?? {}).liveAuth.health;
    if (health === 'configured') return 'api';
    return 'account';
  }
  if (hasCurrentProvider) return 'api';
  return 'none';
}

function accountConnectionTitle(t: TranslateFn, status: AgentStatus | undefined): string {
  const conn = sliceAgentStatus(status ?? {}).effectiveConnection;
  const label =
    (conn.label !== 'unset' ? conn.label.trim() : '')
    || (conn.currentProvider !== 'unset' ? conn.currentProvider.trim() : '');
  if (label && label !== t('chat.connection.unconfiguredLabel')) return label;
  return t('chat.connection.signedIn');
}

export function chatConnectionPickerView(t: TranslateFn, input: {
  primaryAgent: AgentId | null;
  switching?: boolean;
  status?: AgentStatus;
  currentProviderName?: string | null;
  currentProviderModel?: string | null;
  /** Current wallet login for this Agent; wins over leftover provider names. */
  activeLogin?: { title: string; subtitle: string | null } | null;
  leftoverCurrent?: boolean;
  walletReady?: boolean;
}): ChatConnectionPickerView {
  if (!input.primaryAgent) {
    return {
      kind: 'none',
      label: t('chat.connection.switch'),
      subtitle: null,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: t('chat.connection.add'),
    };
  }

  const kind = chatConnectionKind(input.status, Boolean(input.currentProviderName));
  if (input.switching) {
    return {
      kind,
      label: t('chat.connection.switching'),
      subtitle: null,
      currentLoginTitle: kind === 'account' ? accountConnectionTitle(t, input.status) : null,
      currentLoginSubtitle: kind === 'account' ? t('chat.connection.currentLogin') : null,
      emptyHint: kind === 'none' ? t('chat.connection.none') : null,
      manageLabel: kind === 'none' ? t('chat.connection.add') : t('chat.connection.manage'),
    };
  }

  if (input.activeLogin) {
    return {
      kind,
      label: input.activeLogin.title,
      subtitle: input.activeLogin.subtitle,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: t('chat.connection.manage'),
    };
  }

  if (input.leftoverCurrent) {
    return {
      kind,
      label: t('chat.connection.unconfigured'),
      subtitle: null,
      currentLoginTitle: null,
      currentLoginSubtitle: null,
      emptyHint: null,
      manageLabel: t('chat.connection.manage'),
    };
  }

  const allowUnimported = input.walletReady !== false;

  if (kind === 'account') {
    const title = accountConnectionTitle(t, input.status);
    return {
      kind,
      label: title,
      subtitle: null,
      currentLoginTitle: allowUnimported ? title : null,
      currentLoginSubtitle: allowUnimported ? t('chat.connection.currentLogin') : null,
      emptyHint: null,
      manageLabel: t('chat.connection.manage'),
    };
  }

  if (kind === 'api') {
    const title = input.currentProviderName?.trim() || input.status?.effectiveLabel?.trim() || 'API';
    const unimported = allowUnimported && !input.currentProviderName;
    return {
      kind,
      label: title,
      subtitle: input.currentProviderModel?.trim() || null,
      currentLoginTitle: unimported ? title : null,
      currentLoginSubtitle: unimported ? 'API' : null,
      emptyHint: null,
      manageLabel: t('chat.connection.manage'),
    };
  }

  return {
    kind: 'none',
    label: t('chat.connection.unconfigured'),
    subtitle: null,
    currentLoginTitle: null,
    currentLoginSubtitle: null,
    emptyHint: t('chat.connection.none'),
    manageLabel: t('chat.connection.add'),
  };
}

export type ChatConnectionSwitchAction =
  | { type: 'switch-account'; accountId: string }
  | { type: 'switch-provider'; providerId: string }
  | { type: 'bind'; ticketId: string };

export type ChatConnectionOption = {
  ticketId: string;
  title: string;
  subtitle: string | null;
  isCurrent: boolean;
  action: ChatConnectionSwitchAction;
};

export { isLeftoverLocalRouteProvider, leftoverProviderIsCurrent } from '@/lib/leftover-local-route';

/** Native pool row → switch; a login born on another Agent → bind. */
export function chatConnectionSwitchAction(
  ticket: TicketView,
  agentId: AgentId,
): ChatConnectionSwitchAction {
  if (ticket.agentId === agentId) {
    if (ticket.sourceKind === 'account') {
      return { type: 'switch-account', accountId: ticket.sourceId };
    }
    return { type: 'switch-provider', providerId: ticket.sourceId };
  }
  return { type: 'bind', ticketId: ticket.id };
}

function chatTicketSubtitle(
  t: TranslateFn,
  ticket: TicketView,
  route: BindingRoute | undefined,
): string {
  if (route === 'bridge') return t('kind.route.localRoute');
  if (ticket.credentialClass === 'oauth') return t('kind.oauth');
  if (ticket.credentialClass === 'api_key') return t('kind.apikey');
  return t('connections.list.unrecognized');
}

/** Leftover generated providers are not tickets and must not appear here. */
export function chatConnectionOptions(t: TranslateFn, input: {
  wallet: TicketWallet | null | undefined;
  agentId: AgentId | null;
}): ChatConnectionOption[] {
  if (!input.wallet || !input.agentId) return [];
  const agentId = input.agentId;
  const tickets = filterTicketsByAgentUsage(input.wallet, input.wallet.tickets, agentId);
  const active = activeBindingForAgent(input.wallet, agentId);
  return tickets.map((ticket) => {
    const isCurrent = active?.ticket.id === ticket.id;
    return {
      ticketId: ticket.id,
      title: ticket.label,
      subtitle: chatTicketSubtitle(t, ticket, isCurrent ? active?.binding.route : undefined),
      isCurrent,
      action: chatConnectionSwitchAction(ticket, agentId),
    };
  });
}

export function chatShowsUnimportedCurrent(
  options: readonly Pick<ChatConnectionOption, 'isCurrent'>[],
  currentLoginTitle: string | null | undefined,
): boolean {
  return Boolean(currentLoginTitle) && !options.some((option) => option.isCurrent);
}

export function messageStatusLabel(
  t: TranslateFn,
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
      return t('chat.status.generating');
    case 'error':
    case 'failed':
      return t('chat.status.failed');
    case 'cancelled':
      return t('chat.status.cancelled');
    case 'timeout':
      return t('chat.status.timeout');
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

export function conversationTitle(t: TranslateFn, title: string): string {
  return title.trim() ? title : t('chat.title.newConversation');
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
    case 'envNotReady':
      return 'agents';
    case 'unconfiguredAuth':
      return 'connections';
    case 'noCwd':
      return 'pick-directory';
    case 'sendingElsewhere':
      return 'settings';
  }
}

export function blockerCopy(t: TranslateFn, blocker: ChatSendBlocker): {
  text: string;
  primaryAction: string;
  secondaryAction?: string;
} {
  switch (blocker.kind) {
    case 'hiddenAgents':
      return {
        text: t('chat.blocker.hidden'),
        primaryAction: t('chat.blocker.goAgents'),
      };
    case 'envNotReady':
      return {
        text: t('chat.blocker.envNotReady'),
        primaryAction: t('chat.blocker.goAgents'),
      };
    case 'unconfiguredAuth':
      return {
        text: t('chat.blocker.unconfigured'),
        primaryAction: t('chat.blocker.goConnections'),
      };
    case 'noCwd':
      return {
        text: t('chat.blocker.noCwd'),
        primaryAction: t('chat.blocker.setCwd'),
      };
    case 'sendingElsewhere':
      return {
        text: t('chat.blocker.generating', { title: conversationTitle(t, blocker.title) }),
        primaryAction: t('chat.blocker.backToSession'),
        secondaryAction: t('chat.blocker.stop'),
      };
  }
}

/** Composer 正文区：约 1 行起、最多 ~12 行；超出后内部滚动。 */
export const COMPOSER_TEXTAREA_MIN_PX = 56;
export const COMPOSER_TEXTAREA_MAX_PX = 240;

type CssSupports = { supports?(property: string, value: string): boolean };

export function clampComposerTextareaHeight(contentPx: number): number {
  return Math.min(Math.max(contentPx, COMPOSER_TEXTAREA_MIN_PX), COMPOSER_TEXTAREA_MAX_PX);
}

export function composerTextareaOverflowY(contentPx: number): 'auto' | 'hidden' {
  return contentPx > COMPOSER_TEXTAREA_MAX_PX ? 'auto' : 'hidden';
}

/** JS fallback layout after measuring `scrollHeight` (when `field-sizing` is missing). */
export function composerTextareaMeasuredStyle(contentPx: number): {
  height: string;
  overflowY: 'auto' | 'hidden';
} {
  return {
    height: `${clampComposerTextareaHeight(contentPx)}px`,
    overflowY: composerTextareaOverflowY(contentPx),
  };
}

/** Pass `css` in tests; omit to read the runtime `CSS` object. */
export function composerUsesCssFieldSizing(css?: CssSupports | null): boolean {
  const api = css === undefined ? (typeof CSS === 'undefined' ? undefined : CSS) : css ?? undefined;
  return typeof api?.supports === 'function' && api.supports('field-sizing', 'content');
}

/** 对话记录与 composer 共用的主列宽，与 Settings 共用 `pageRhythm.readingColumn`。 */
export const chatMainColumnClass = pageRhythm.readingColumn;

/** 对话记录与输入壳外侧同一圈 16px 缝（水平再叠 `chatChromeX`）。 */
export const chatStageClass = 'flex min-h-0 flex-1 flex-col py-4';

/**
 * 空转录与 composer 周围同色（canvas）；有消息后对话记录列与输入壳同色（panel）。
 */
export function chatTranscriptSurfaceClass(hasMessages: boolean): string {
  return hasMessages ? 'bg-panel' : 'bg-canvas';
}
