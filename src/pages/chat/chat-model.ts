/**
 * Chat 页纯函数：会话分组 / 发送前置 / 展示文案。
 * 不 import React、不碰 lib/api。
 */
import { agentDisplayName } from '@/config/agents';
import {
  formatLocalRouteLabel,
  isInternalGeneratedProvider,
} from '@/lib/backend/contracts/agent-connection';
import type { TranslateFn } from '@/lib/i18n';
import { processPhaseLabel, type AgentProcessView } from '@/lib/chat-process';
import { nativeResumeCommand } from '@/lib/session-resume';
import type {
  Account,
  AgentId,
  AgentStatus,
  ChatMessage,
  ChatMessageStatus,
  Conversation,
  Provider,
} from '@/lib/types';
import { extractModel, type TurnGroup } from './chat-format';

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
  return status.envReady !== false;
}

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
  return Boolean(
    status?.installed && !status.hidden && agentHasConfiguredAuth(status) && agentChatEnvReady(status),
  );
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
  if (status?.effectiveKind === 'account') return 'account';
  if (status?.effectiveKind === 'api') return 'api';
  if (agentHasConfiguredAuth(status)) {
    if (status?.authLabel === 'API' || status?.authHealth === 'configured') return 'api';
    return 'account';
  }
  if (hasCurrentProvider) return 'api';
  return 'none';
}

function accountConnectionTitle(t: TranslateFn, status: AgentStatus | undefined): string {
  const label = status?.effectiveLabel?.trim() || status?.currentProvider?.trim();
  if (label && label !== t('chat.connection.unconfiguredLabel')) return label;
  return t('chat.connection.signedIn');
}

export function chatConnectionPickerView(t: TranslateFn, input: {
  primaryAgent: AgentId | null;
  switching?: boolean;
  status?: AgentStatus;
  currentProviderName?: string | null;
  currentProviderModel?: string | null;
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

  if (kind === 'account') {
    const title = accountConnectionTitle(t, input.status);
    return {
      kind,
      label: title,
      subtitle: null,
      currentLoginTitle: title,
      currentLoginSubtitle: t('chat.connection.currentLogin'),
      emptyHint: null,
      manageLabel: t('chat.connection.manage'),
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

export type ChatConnectionOptionKind = 'account' | 'provider';

export type ChatConnectionOption = {
  kind: ChatConnectionOptionKind;
  id: string;
  title: string;
  subtitle: string | null;
  isCurrent: boolean;
};

const AGENTHUB_BRIDGE_SLUG = /agenthub_[^\s"'\\]*_bridge/i;

/** Leftover generated 本机路由 rows — never labeled 官方登录. */
export function isLeftoverLocalRouteProvider(
  provider: Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>,
): boolean {
  if (isInternalGeneratedProvider(provider)) return true;
  const haystack = `${provider.id}\n${provider.name}\n${provider.preset ?? ''}\n${provider.configText ?? ''}`;
  return haystack.includes('本机路由')
    || AGENTHUB_BRIDGE_SLUG.test(haystack)
    || haystack.includes('127.0.0.1');
}

export function officialOauthAccountTitle(account: Pick<Account, 'email' | 'label'>): string {
  const email = account.email?.trim();
  if (email) return email;
  return account.label;
}

function officialOauthDedupeKey(account: Pick<Account, 'email' | 'identityLabel' | 'label' | 'id'>): string {
  const key = account.email?.trim() || account.identityLabel?.trim() || account.label.trim() || account.id;
  return key.toLowerCase();
}

export function leftoverProviderIsCurrent(providers: readonly Provider[]): boolean {
  return providers.some((provider) => provider.isCurrent && isLeftoverLocalRouteProvider(provider));
}

function officialOauthWinners(accounts: readonly Account[]): Account[] {
  const winners: Account[] = [];
  const indexByKey = new Map<string, number>();
  for (const account of accounts) {
    if (account.kind !== 'oauth') continue;
    const key = officialOauthDedupeKey(account);
    const existing = indexByKey.get(key);
    if (existing == null) {
      indexByKey.set(key, winners.length);
      winners.push(account);
      continue;
    }
    if (account.isCurrent && !winners[existing].isCurrent) {
      winners[existing] = account;
    }
  }
  return winners;
}

/**
 * Chat switch-connection options: official oauth and leftover local-route as
 * separate entries. Leftover current wins the checkmark so official rows stay clickable.
 */
export function chatConnectionOptions(t: TranslateFn, input: {
  accounts: readonly Account[];
  providers: readonly Provider[];
  connectionKind?: ChatConnectionPickerKind;
}): ChatConnectionOption[] {
  const leftoverCurrent = leftoverProviderIsCurrent(input.providers);
  const preferAccount = input.connectionKind === 'account' && !leftoverCurrent;
  const options: ChatConnectionOption[] = [];
  for (const account of officialOauthWinners(input.accounts)) {
    options.push({
      kind: 'account',
      id: account.id,
      title: officialOauthAccountTitle(account),
      subtitle: t('kind.oauth'),
      isCurrent: leftoverCurrent ? false : account.isCurrent,
    });
  }
  for (const provider of input.providers) {
    const leftover = isLeftoverLocalRouteProvider(provider);
    options.push({
      kind: 'provider',
      id: provider.id,
      title: leftover ? formatLocalRouteLabel(undefined, t) : provider.name,
      subtitle: leftover ? null : extractModel(provider.configText),
      isCurrent: leftoverCurrent ? leftover && provider.isCurrent : preferAccount ? false : provider.isCurrent,
    });
  }
  return options;
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
