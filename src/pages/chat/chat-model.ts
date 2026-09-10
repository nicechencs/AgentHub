/**
 * Chat 页纯函数：会话分组 / 发送前置 / 展示文案。
 * 不 import React、不碰 lib/api。
 */
import { pageRhythm } from '@/components/layout/page-rhythm';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
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
  AgentKey,
  AgentStatus,
  ChatMessage,
  ChatMessageStatus,
  Conversation,
} from '@/lib/types';
import { relativeTime, type TurnGroup } from './chat-format';
import { streamingStatusKey } from './chat-streaming';

export type ChatSendBlocker =
  | { kind: 'hiddenAgents'; agentIds: AgentKey[] }
  | { kind: 'envNotReady'; agentIds: AgentKey[] }
  | { kind: 'unconfiguredAuth'; agentIds: AgentKey[] }
  | { kind: 'statusUnknown' }
  | { kind: 'noCwd' };

export type ChatAgentPickerReason = 'noAuth' | 'envNotReady';

export type ChatAgentPickerRow = {
  id: AgentKey;
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
  catalogIds: readonly AgentKey[];
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

export function conversationCwdMissing(
  conversation: Pick<Conversation, 'cwd' | 'cwdMissing'>,
): boolean {
  return Boolean(conversation.cwd?.trim() && conversation.cwdMissing);
}

export function canRebindConversationCwd(
  conversation: Pick<Conversation, 'cwd' | 'cwdMissing'>,
  runtimeLocked: boolean,
): boolean {
  return !runtimeLocked || conversationCwdMissing(conversation);
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
  hiddenIds: Set<AgentKey>;
  envNotReadyIds?: Set<AgentKey>;
  unconfiguredAuthIds?: Set<AgentKey>;
  agentsReady?: boolean;
}): ChatSendBlocker[] {
  const out: ChatSendBlocker[] = [];
  if (input.agentsReady === false) {
    out.push({ kind: 'statusUnknown' });
    return out;
  }
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
  return out;
}

/** Normalize restored in-flight ids. Null/legacy single id must not throw. */
export function incomingSendingIds(ids: unknown): string[] {
  if (typeof ids === 'string') return ids ? [ids] : [];
  if (!Array.isArray(ids)) return [];
  const out: string[] = [];
  const seen = new Set<string>();
  for (const id of ids) {
    if (typeof id !== 'string' || !id || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

/** Keep page-local sending ids that still exist in the conversation list. */
export function liveSendingIds(
  sendingIds: readonly string[] | null | undefined,
  conversations: readonly Pick<Conversation, 'id'>[],
): string[] {
  if (!sendingIds?.length) return [];
  const known = new Set(conversations.map((conversation) => conversation.id));
  return sendingIds.filter((id) => known.has(id));
}

/** Agents with at least one in-flight send. Connection/model switches stay locked for them. */
export function busyAgentsForSends(
  conversations: readonly Pick<Conversation, 'id' | 'agentIds'>[],
  sendingIds: readonly string[],
): Set<AgentKey> {
  const sending = new Set(sendingIds);
  const agents = new Set<AgentKey>();
  for (const conversation of conversations) {
    if (!sending.has(conversation.id)) continue;
    const agentId = conversation.agentIds[0];
    if (agentId) agents.add(agentId);
  }
  return agents;
}

/** Persist the leaving session's draft and restore the focused session's draft. */
export function draftForFocusedConversation(
  drafts: Map<string, string>,
  fromId: string | null,
  toId: string,
  currentDraft: string,
): string {
  if (fromId) drafts.set(fromId, currentDraft);
  if (fromId === toId) return currentDraft;
  return drafts.get(toId) ?? '';
}

/**
 * Chat headless 自动批准的真实效果，对齐各 adapter `build_run_spec`。
 * 不是 capability 标牌：Kimi/DSH 在 TUI 里有 yolo，但 -p / headless 不会加上。
 */
export type AutoApproveEffect = 'skip' | 'project-trust' | 'none';

export function autoApproveEffect(agentId: AgentKey | null | undefined): AutoApproveEffect {
  switch (agentId) {
    case 'claude':
    case 'codex':
    case 'grok':
    case 'workbuddy':
    case 'cursor':
    case 'kiro':
      return 'skip';
    case 'pi':
      return 'project-trust';
    default:
      return 'none';
  }
}

export function autoApproveActive(
  allowDangerous: boolean,
  agentId: AgentKey | null | undefined,
): boolean {
  return allowDangerous && autoApproveEffect(agentId) !== 'none';
}

export function autoApproveHint(
  t: TranslateFn,
  effect: AutoApproveEffect,
  agentId?: AgentKey | null,
): string {
  if (agentId === 'kiro') {
    return effect === 'skip' ? t('chat.kiro.permissionFullHint') : t('chat.autoApprove.none');
  }
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
  agentId: AgentKey | null | undefined,
): { text: string; warning: boolean } {
  const effect = autoApproveEffect(agentId);
  if (!allowDangerous) {
    return { text: '', warning: false };
  }
  if (effect === 'skip') {
    return {
      text: agentId === 'kiro' ? t('chat.autoApprove.footerKiroFull') : t('chat.autoApprove.footerSkip'),
      warning: true,
    };
  }
  if (effect === 'project-trust') {
    return { text: t('chat.autoApprove.footerTrust'), warning: true };
  }
  return { text: t('chat.autoApprove.footerNone'), warning: false };
}

export function autoApproveConfirmCopy(
  t: TranslateFn,
  effect: AutoApproveEffect,
  agentId?: AgentKey | null,
): string {
  if (agentId === 'kiro') {
    return t('chat.autoApprove.confirmKiroFull');
  }
  if (effect === 'project-trust') {
    return t('chat.autoApprove.confirmTrust');
  }
  return t('chat.autoApprove.confirmSkip');
}

/** 单选：点当前项不变；点其他项替换。无法跳过确认的 Agent 会清掉已开的自动批准。 */
export function selectConversationAgent(input: {
  currentIds: AgentKey[];
  nextId: AgentKey;
  allowDangerous: boolean;
}): { agentIds: AgentKey[]; allowDangerous?: boolean } | null {
  if (input.currentIds.length === 1 && input.currentIds[0] === input.nextId) {
    return null;
  }
  const patch: { agentIds: AgentKey[]; allowDangerous?: boolean } = {
    agentIds: [input.nextId],
  };
  if (input.allowDangerous && autoApproveEffect(input.nextId) === 'none') {
    patch.allowDangerous = false;
  }
  return patch;
}

/**
 * One-shot migrate: multi-agent conversations → keep first agent only.
 * Product intent is single-agent selection (see `selectConversationAgent`);
 * this collapses any legacy multi-agent rows without re-running on every open.
 */
export function singleAgentConversationPatch(
  agentIds: AgentKey[],
): { agentIds: AgentKey[] } | null {
  if (agentIds.length <= 1) return null;
  return { agentIds: [agentIds[0]] };
}

export function newConversationDefaults(
  active: Conversation | null,
  agentStatus: AgentStatus[],
): { agentIds: AgentKey[]; cwd: string | null } {
  const hidden = new Set(agentStatus.filter((a) => a.hidden).map((a) => a.agentId));
  const uninstalled = new Set(
    agentStatus.filter((a) => a.installed === false).map((a) => a.agentId),
  );
  const fallback = agentStatus.find((a) => isChatAgentSelectable(a))?.agentId;
  const fallbackIds: AgentKey[] = fallback ? [fallback] : [];

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
  agentIds: AgentKey[];
  primaryAgent?: AgentKey | null;
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
  const label = conn.label !== 'unset' ? conn.label.trim() : '';
  if (label && label !== t('chat.connection.unconfiguredLabel')) return label;
  return t('chat.connection.signedIn');
}

/** Pool `none` still writes effectiveLabel 「未配置」; never show that as an API title. */
function apiConnectionTitle(
  t: TranslateFn,
  currentProviderName: string | null | undefined,
  status: AgentStatus | undefined,
): string {
  const unconfigured = t('chat.connection.unconfiguredLabel');
  for (const raw of [currentProviderName, status?.effectiveLabel]) {
    const label = raw?.trim() ?? '';
    if (label && label !== unconfigured) return label;
  }
  return 'API';
}

export function chatConnectionPickerView(t: TranslateFn, input: {
  primaryAgent: AgentKey | null;
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
    const title = apiConnectionTitle(t, input.currentProviderName, input.status);
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
  agentId: AgentKey,
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
  agentId: AgentKey | null;
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
  hasContent = false,
): string | null {
  // 过程机更细（排队/启动）；生成中首字前「正在想」，有正文后「正在写」
  if (process && (status === 'running' || !status)) {
    if (process.phase === 'queued' || process.phase === 'starting') {
      return processPhaseLabel(process.phase, t);
    }
    if (process.phase === 'running') {
      return t(streamingStatusKey(process, hasContent));
    }
  }
  switch (status) {
    case 'running':
      return t(streamingStatusKey(process, hasContent));
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

/** Esc stops the in-flight turn unless a dialog, preview, or IME already owns it. */
export function chatEscapeShouldCancel(input: {
  key: string;
  sending: boolean;
  canceling: boolean;
  previewOpen: boolean;
  overlayOpen: boolean;
  defaultPrevented: boolean;
  composing?: boolean;
}): boolean {
  if (input.key !== 'Escape') return false;
  if (input.composing || input.defaultPrevented || input.overlayOpen || input.previewOpen) {
    return false;
  }
  return input.sending && !input.canceling;
}

/** Enter sends; Shift+Enter inserts a newline; IME composition must not send. */
export function composerEnterShouldSend(input: {
  key: string;
  shiftKey: boolean;
  isComposing?: boolean;
  nativeEvent?: { isComposing?: boolean; keyCode?: number };
}): boolean {
  if (input.key !== 'Enter' || input.shiftKey) return false;
  if (input.isComposing || input.nativeEvent?.isComposing) return false;
  if (input.nativeEvent?.keyCode === 229) return false;
  return true;
}

export { dialogEnterShouldConfirm } from '@/lib/dialog-enter';

export type ComposerNativeEditChord = 'selectAll' | 'copy' | 'cut' | 'paste';

/** Ctrl/Cmd+A/C/X/V in a field must reach the native edit action. */
export function composerNativeEditChord(input: {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}): ComposerNativeEditChord | null {
  if (input.altKey || input.shiftKey) return null;
  if (!(input.metaKey || input.ctrlKey)) return null;
  const letter = input.key.length === 1 ? input.key.toLowerCase() : '';
  if (letter === 'a' || input.code === 'KeyA') return 'selectAll';
  if (letter === 'c' || input.code === 'KeyC') return 'copy';
  if (letter === 'x' || input.code === 'KeyX') return 'cut';
  if (letter === 'v' || input.code === 'KeyV') return 'paste';
  return null;
}

function chatModChordMatchesLetter(
  input: { key: string; code?: string; metaKey: boolean; ctrlKey: boolean },
  letter: 'n' | 'k',
  code: 'KeyN' | 'KeyK',
): boolean {
  if (!(input.metaKey || input.ctrlKey)) return false;
  const upper = letter.toUpperCase();
  if (input.key === letter || input.key === upper) return true;
  // WebKitGTK/IME may report Unidentified or a control char; the physical code stays stable.
  return input.code === code;
}

export function chatModKShouldFocusHistory(input: {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  overlayOpen: boolean;
}): boolean {
  if (input.overlayOpen || input.altKey || input.shiftKey) return false;
  return chatModChordMatchesLetter(input, 'k', 'KeyK');
}

/** Cmd/Ctrl+N starts a new chat (same modifier pattern as Ctrl+K). */
export function chatModNShouldStartNewChat(input: {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  overlayOpen: boolean;
}): boolean {
  if (input.overlayOpen || input.altKey || input.shiftKey) return false;
  return chatModChordMatchesLetter(input, 'n', 'KeyN');
}

/** True when the event target is a field that should keep typed characters. */
export function chatKeyTargetIsField(target: EventTarget | null): boolean {
  if (!target || typeof target !== 'object') return false;
  const el = target as { tagName?: string; isContentEditable?: boolean };
  const tag = el.tagName?.toUpperCase();
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return Boolean(el.isContentEditable);
}

/** `?` opens the shortcut overview when not typing in a field. */
export function chatQuestionShouldOpenShortcuts(input: {
  key: string;
  code?: string;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  overlayOpen: boolean;
  typingInField: boolean;
}): boolean {
  if (input.overlayOpen || input.typingInField || input.altKey || input.metaKey || input.ctrlKey) {
    return false;
  }
  if (input.key === '?') return true;
  // US `?` is Shift+/. Some webviews report key:'/' or Unidentified instead of '?'.
  // Do not match code === 'Slash' alone (a letter key must stay a letter).
  if (input.shiftKey && input.key === '/') return true;
  return Boolean(
    input.shiftKey && (input.key === 'Unidentified' || input.key === '') && input.code === 'Slash',
  );
}

/**
 * Page-level Chat chords. Ctrl/Cmd+N still fires when the target is the composer
 * textarea; `?` does not (the field keeps the character).
 */
export function chatPageShortcutAction(input: {
  key: string;
  code?: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  overlayOpen: boolean;
  target: EventTarget | null;
}): 'history' | 'newChat' | 'overview' | null {
  if (composerNativeEditChord(input)) return null;
  if (chatModKShouldFocusHistory(input)) return 'history';
  if (chatModNShouldStartNewChat(input)) return 'newChat';
  if (
    chatQuestionShouldOpenShortcuts({
      ...input,
      typingInField: chatKeyTargetIsField(input.target),
    })
  ) {
    return 'overview';
  }
  return null;
}

export function visibleAgentDots(agentIds: AgentKey[]): { shown: AgentKey[]; extra: number } {
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
  agentId: AgentKey;
  status: ChatMessageStatus;
  durationMs: number;
  messageId: string;
}> {
  return agents
    .filter((m): m is ChatMessage & { agentId: AgentKey } => Boolean(m.agentId))
    .map((m) => ({
      agentId: m.agentId,
      status: m.status,
      durationMs: m.durationMs,
      messageId: m.id,
    }));
}

const PATH_TOKEN = /(?:[A-Za-z]:)?(?:[\\/][^\s\\/`'"]+)+/g;
const WEAK_LEAD = /^(?:请(?:帮我)?在|请|in|at)\s+/i;
const WEAK_ONLY = /^(?:请(?:帮我)?在|请|in|at|only)$/i;
const TITLE_CLIP = 24;

/** Drop filesystem paths so a prompt like "请在 /tmp/foo 检查问题" keeps 检查问题. */
export function conversationSemanticTitle(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';
  let next = trimmed.replace(/`[^`]+`/g, ' ').replace(PATH_TOKEN, ' ');
  next = next.replace(/\s+/g, ' ').trim().replace(WEAK_LEAD, '').trim();
  if (!next || WEAK_ONLY.test(next)) return '';
  return next.length > TITLE_CLIP ? `${next.slice(0, TITLE_CLIP)}…` : next;
}

/** First send: store a semantic title, never a path-first clip of the prompt. */
export function titleFromPrompt(prompt: string): string {
  return conversationSemanticTitle(prompt);
}

export function conversationTitle(t: TranslateFn, title: string): string {
  const semantic = conversationSemanticTitle(title);
  if (semantic) return semantic;
  return t('chat.title.newConversation');
}

/** Empty title and no official session means this row has not been sent yet. */
export function isBlankConversationDraft(
  conversation: Pick<Conversation, 'title' | 'nativeSessionId'>,
): boolean {
  return !conversation.title.trim() && !conversation.nativeSessionId;
}

export function conversationAgentLine(agentIds: readonly AgentKey[]): string {
  if (agentIds.length === 0) return '';
  if (agentIds.length === 1) return agentDisplayName(agentIds[0]);
  if (agentIds.length === 2) {
    return `${agentDisplayName(agentIds[0])} · ${agentDisplayName(agentIds[1])}`;
  }
  return `${agentDisplayName(agentIds[0])} +${agentIds.length - 1}`;
}

/** Hover details for a one-line history row, excluding Agent (shown as logos). */
export function conversationRailHint(
  conversation: Pick<
    Conversation,
    'agentIds' | 'cwd' | 'updatedAt' | 'title' | 'nativeSessionId'
  >,
  t: TranslateFn,
): string {
  const rawTitle = conversation.title.trim();
  const displayTitle = conversationTitle(t, conversation.title);
  const parts = [
    rawTitle && rawTitle !== displayTitle ? rawTitle : '',
    conversation.cwd?.trim() || t('chat.cwd.unset'),
    relativeTime(conversation.updatedAt, t),
  ].filter(Boolean);
  if (isBlankConversationDraft(conversation)) parts.push(t('chat.rail.draft'));
  if (conversation.nativeSessionId) {
    parts.push(t('chat.header.nativeSession', { id: conversation.nativeSessionId }));
  }
  return parts.join(' · ');
}

/** Selected history-row mark: first Agent brand, else the nav accent. */
export function conversationRailMarkColor(agentIds: readonly AgentKey[]): string {
  const id = agentIds[0];
  return id ? resolveAgentMeta(id).color : 'var(--accent)';
}

/** Selected history-row fill: a visible wash of the Agent mark on canvas. */
export function conversationRailSelectedFill(agentIds: readonly AgentKey[]): string {
  return `color-mix(in srgb, ${conversationRailMarkColor(agentIds)} 28%, var(--bg-canvas))`;
}

export type ChatBlockerPrimaryTarget =
  | 'agents'
  | 'connections'
  | 'pick-directory'
  | 'retry';

export function blockerPrimaryTarget(
  blocker: Pick<ChatSendBlocker, 'kind'>,
): ChatBlockerPrimaryTarget {
  switch (blocker.kind) {
    case 'hiddenAgents':
    case 'envNotReady':
      return 'agents';
    case 'unconfiguredAuth':
      return 'connections';
    case 'statusUnknown':
      return 'retry';
    case 'noCwd':
      return 'pick-directory';
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
    case 'statusUnknown':
      return {
        text: t('chat.blocker.statusUnknown'),
        primaryAction: t('chat.blocker.retryStatus'),
      };
    case 'noCwd':
      return {
        text: t('chat.blocker.noCwd'),
        primaryAction: t('chat.blocker.setCwd'),
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

/** 对话记录与 composer 共用的主列宽（`pageRhythm.readingColumn`）。 */
export const chatMainColumnClass = pageRhythm.readingColumn;

/** 对话记录与输入壳外侧上下 16px；水平缝由页面上的 `chatChromeX` 提供，与页边 12px 对齐。 */
export const chatStageClass = 'flex min-h-0 flex-1 flex-col py-4';

/** Transcript reading surface. Transparent so the chat column canvas shows through. Do not paint bg-panel here. */
export const chatTranscriptSurfaceClass = 'bg-transparent';
