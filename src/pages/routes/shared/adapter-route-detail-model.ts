/**
 * Pure view-model for the route-detail source node and its upstream channel.
 * No React, no IO. Endpoint mapping rows live in route-graph-model.
 */
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { MessageKey, MessageParams, TranslateFn } from '@/lib/i18n';
import { ROUTE_ENDPOINT_PENDING_PORT } from '@/lib/route-endpoints';
import type { AgentKey } from '@/lib/types';
import { readCreateRouteCapabilities, type CreateRouteTarget } from './create-route-flow';
import { resolveAdapterProfileSource } from './adapter-view-model';

export type UpstreamChannel =
  | 'openai_chat'
  | 'anthropic_messages'
  | 'codex_responses'
  | 'grok_responses'
  | 'unknown';

export type RouteDetailEdgeTarget = 'claude' | 'codex' | 'grok' | 'kimi' | 'dsh';

export type RouteHopKind = 'passthrough' | 'convert' | 'forward';

const APPLIED_TARGETS = new Set<string>(['claude', 'codex', 'grok', 'kimi', 'dsh']);

const DETAIL_COPY = {
  hopPassthrough: '直通上游',
  hopConvert: '转换 → {channel}',
  hopForward: '转发',
  channelOpenaiChat: '上游 Chat 接口',
  channelAnthropicMessages: '上游 Messages',
  channelCodexResponses: '上游 Codex Responses',
  channelGrokResponses: '上游 Grok Responses',
  channelUnknown: '上游',
  modelsOnly: '仅放行：{models}（其余模型将被拒绝）',
  modelsAny: '跟随客户端请求的模型',
  stoppedHint: '已停止——客户端暂时无法使用以下地址',
  portPending: '端口分配中',
  hostPortPending: '127.0.0.1 · 端口分配中',
  sourceDeletedHint: '来源登录已删除，路由仅可查看或解除绑定',
  copyPortPending: '端口分配后可复制',
} as const;

function applyParams(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (all, name: string) => (
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : all
  ));
}

function detailText(
  t: TranslateFn | undefined,
  key: string,
  fallback: string,
  params?: MessageParams,
): string {
  if (!t) return applyParams(fallback, params);
  return t(key as MessageKey, params);
}

function withoutPendingPortLiteral(value: string, pendingLabel: string): string {
  return value.includes(ROUTE_ENDPOINT_PENDING_PORT)
    ? value.split(ROUTE_ENDPOINT_PENDING_PORT).join(pendingLabel)
    : value;
}

function isUsableUrl(url: string | null | undefined): boolean {
  return Boolean(url?.trim());
}

/** Detect Anthropic Messages vs OpenAI Chat from URL (local-route-endpoints.md). */
export function detectUpstreamChannelFromUrl(url: string): UpstreamChannel {
  const normalized = url.trim().toLowerCase();
  if (!normalized) return 'unknown';
  if (normalized.includes('/anthropic') || normalized.includes('api.anthropic.com')) {
    return 'anthropic_messages';
  }
  return 'openai_chat';
}

/** OAuth/subscription fallback from profile.mode + source agentId when no usable URL. */
export function detectUpstreamChannelFromCredential(input: {
  mode: 'api' | 'oauth';
  sourceAgentId?: string | null;
}): UpstreamChannel {
  const agent = input.sourceAgentId?.trim() ?? '';
  if (input.mode === 'oauth') {
    if (agent === 'claude' || agent === 'anthropic') return 'anthropic_messages';
    if (agent === 'codex' || agent === 'openai' || agent === 'openai-codex') return 'codex_responses';
    if (agent === 'grok' || agent === 'xai') return 'grok_responses';
    if (agent === 'kimi' || agent === 'dsh') return 'openai_chat';
    return 'unknown';
  }
  if (agent === 'kimi' || agent === 'dsh') return 'openai_chat';
  return 'unknown';
}

function isEdgeTarget(value: string): value is RouteDetailEdgeTarget {
  return APPLIED_TARGETS.has(value);
}

export type RouteWriteTruth = {
  /** Target agent → the provider id that is currently live (`isCurrent`). */
  currentProviderByAgent: Readonly<Partial<Record<string, string>>>;
  /** Local-bridge profile ids whose local entry is running. */
  runningProfileIds: ReadonlySet<string>;
};

export type RouteWriteNote = 'stopped' | 'rewritten' | null;

/** Current provider id per agent. Only provider rows — generated local writes live there. */
export function currentProviderIdsFromEntries(
  entries: readonly { source: string; agentId: string; isCurrent: boolean; id: string }[],
): Partial<Record<string, string>> {
  const out: Partial<Record<string, string>> = {};
  for (const entry of entries) {
    if (entry.source !== 'provider' || !entry.isCurrent) continue;
    const id = entry.id.trim();
    if (!id) continue;
    out[entry.agentId] = id;
  }
  return out;
}

/** Profile ids whose local entry is actually running. */
export function runningAdapterProfileIds(
  statuses: Readonly<Record<string, { state?: string | null } | undefined>>,
): Set<string> {
  const ids = new Set<string>();
  for (const [id, status] of Object.entries(statuses)) {
    if (status?.state === 'running') ids.add(id);
  }
  return ids;
}

export function routeWriteTruthFrom(
  entries: readonly { source: string; agentId: string; isCurrent: boolean; id: string }[],
  statuses: Readonly<Record<string, { state?: string | null } | undefined>>,
): RouteWriteTruth {
  return {
    currentProviderByAgent: currentProviderIdsFromEntries(entries),
    runningProfileIds: runningAdapterProfileIds(statuses),
  };
}

/**
 * 「已写入」= this route's generated provider is the agent's current provider
 * **and** that local entry is running. A leftover stamp on a stopped or
 * rewritten route is not applied.
 */
export function appliedTargetsFromProfiles(
  profiles: readonly Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>[],
  source: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
  writeTruth?: RouteWriteTruth,
): ReadonlySet<RouteDetailEdgeTarget> {
  const applied = new Set<RouteDetailEdgeTarget>();
  for (const profile of profiles) {
    if (writeStateForProfile(profile, source, writeTruth).applied && isEdgeTarget(profile.targetAgentId)) {
      applied.add(profile.targetAgentId);
    }
  }
  return applied;
}

export function writeStateForProfile(
  profile: Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>,
  source: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
  writeTruth?: RouteWriteTruth,
): { applied: boolean; writeNote: RouteWriteNote } {
  if (profile.route !== 'local_bridge') return { applied: false, writeNote: null };
  if (profile.sourceKind !== source.sourceKind || profile.sourceId !== source.sourceId) {
    return { applied: false, writeNote: null };
  }
  const generated = profile.generatedProviderId?.trim() ?? '';
  if (!generated) return { applied: false, writeNote: null };
  if (!isEdgeTarget(profile.targetAgentId)) return { applied: false, writeNote: null };
  if (!writeTruth) return { applied: false, writeNote: null };
  const currentId = writeTruth.currentProviderByAgent[profile.targetAgentId]?.trim() ?? '';
  const running = writeTruth.runningProfileIds.has(profile.id);
  if (currentId && currentId === generated && running) {
    return { applied: true, writeNote: null };
  }
  if (currentId && currentId === generated && !running) {
    return { applied: false, writeNote: 'stopped' };
  }
  if (generated && currentId && currentId !== generated) {
    return { applied: false, writeNote: 'rewritten' };
  }
  return { applied: false, writeNote: null };
}

function readSourceBaseUrl(configText: string | undefined): string {
  try {
    const parsed = JSON.parse(configText ?? '{}') as {
      baseURL?: unknown;
      baseUrl?: unknown;
      base_url?: unknown;
    };
    if (typeof parsed.baseURL === 'string' && parsed.baseURL.trim()) return parsed.baseURL.trim();
    if (typeof parsed.baseUrl === 'string' && parsed.baseUrl.trim()) return parsed.baseUrl.trim();
    if (typeof parsed.base_url === 'string' && parsed.base_url.trim()) return parsed.base_url.trim();
    return '';
  } catch {
    return '';
  }
}

function uniqueUrls(urls: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of urls) {
    const url = raw.trim();
    if (!url || seen.has(url)) continue;
    seen.add(url);
    out.push(url);
  }
  return out;
}

function matchSourceEntry(
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
  entries: readonly ConnectionEntry[],
): ConnectionEntry | undefined {
  return entries.find((entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId);
}

function resolveSourceChannel(input: {
  missing: boolean;
  mode: 'api' | 'oauth';
  sourceAgentId: AgentKey | null;
  baseUrl: string;
  upstreamUrls: readonly string[];
}): UpstreamChannel {
  if (input.missing) return 'unknown';
  const url = input.baseUrl.trim() || input.upstreamUrls.find(isUsableUrl) || '';
  if (isUsableUrl(url)) return detectUpstreamChannelFromUrl(url);
  return detectUpstreamChannelFromCredential({
    mode: input.mode,
    sourceAgentId: input.sourceAgentId,
  });
}

export type RouteDetailSourceView = {
  title: string;
  agentId: AgentKey | null;
  missing: boolean;
  credentialMode: 'api' | 'oauth';
  /** Display base upstream URL (may be truncated by UI). Empty if unknown. */
  baseUrl: string;
  /** Full upstream URLs for diagnostics (unique). */
  upstreamUrls: string[];
  /** Best-effort channel for left node; unknown if cannot derive. */
  channel: UpstreamChannel;
};

export function buildRouteDetailSourceView(input: {
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name' | 'mode'>;
  entries: readonly ConnectionEntry[];
}): RouteDetailSourceView {
  const resolved = resolveAdapterProfileSource(input.profile, input.entries);
  const entry = matchSourceEntry(input.profile, input.entries);
  const configText = entry?.provider?.configText;
  const caps = readCreateRouteCapabilities(configText);
  const baseUrl = resolved.missing ? '' : readSourceBaseUrl(configText);
  const upstreamUrls = resolved.missing
    ? []
    : uniqueUrls([baseUrl, ...caps.endpoints.map((row) => row.url)]);
  return {
    title: resolved.title,
    agentId: resolved.agentId,
    missing: resolved.missing,
    credentialMode: input.profile.mode,
    baseUrl,
    upstreamUrls,
    channel: resolveSourceChannel({
      missing: resolved.missing,
      mode: input.profile.mode,
      sourceAgentId: resolved.agentId,
      baseUrl,
      upstreamUrls,
    }),
  };
}

function hopFor(
  endpointId: 'messages' | 'responses' | 'chat_completions',
  channel: UpstreamChannel,
): RouteHopKind {
  if (channel === 'unknown') return 'forward';
  if (endpointId === 'messages' && channel === 'anthropic_messages') return 'passthrough';
  if (endpointId === 'responses' && (channel === 'codex_responses' || channel === 'grok_responses')) {
    return 'passthrough';
  }
  if (endpointId === 'chat_completions' && channel === 'openai_chat') return 'passthrough';
  return 'convert';
}

/** Hop for one downstream surface against an upstream channel; unknown → forward. */
export function hopForTestable(
  endpointId: 'messages' | 'responses' | 'chat_completions',
  channel: UpstreamChannel,
): RouteHopKind {
  return hopFor(endpointId, channel);
}

export function upstreamChannelLabel(channel: UpstreamChannel, t?: TranslateFn): string {
  if (channel === 'openai_chat') {
    return detailText(t, 'routes.panel.channel.openaiChat', DETAIL_COPY.channelOpenaiChat);
  }
  if (channel === 'anthropic_messages') {
    return detailText(t, 'routes.panel.channel.anthropicMessages', DETAIL_COPY.channelAnthropicMessages);
  }
  if (channel === 'codex_responses') {
    return detailText(t, 'routes.panel.channel.codexResponses', DETAIL_COPY.channelCodexResponses);
  }
  if (channel === 'grok_responses') {
    return detailText(t, 'routes.panel.channel.grokResponses', DETAIL_COPY.channelGrokResponses);
  }
  return detailText(t, 'routes.panel.channel.unknown', DETAIL_COPY.channelUnknown);
}

export function routeHopLabel(hop: RouteHopKind, channel: UpstreamChannel, t?: TranslateFn): string {
  if (hop === 'passthrough') {
    return detailText(t, 'routes.panel.hop.passthrough', DETAIL_COPY.hopPassthrough);
  }
  if (hop === 'convert') {
    return detailText(t, 'routes.panel.hop.convert', DETAIL_COPY.hopConvert, {
      channel: upstreamChannelLabel(channel, t),
    });
  }
  return detailText(t, 'routes.panel.hop.forward', DETAIL_COPY.hopForward);
}

export function routeModelsSummary(models: readonly string[], t?: TranslateFn): string {
  if (models.length === 0) {
    return detailText(t, 'routes.panel.models.any', DETAIL_COPY.modelsAny);
  }
  return detailText(t, 'routes.panel.models.only', DETAIL_COPY.modelsOnly, {
    models: models.join(', '),
  });
}

export function routeSourceDeletedHint(t?: TranslateFn): string {
  return detailText(t, 'routes.panel.sourceDeletedHint', DETAIL_COPY.sourceDeletedHint);
}

export function routeCopyPortPendingLabel(t?: TranslateFn): string {
  return detailText(t, 'routes.panel.copyPortPending', DETAIL_COPY.copyPortPending);
}

/** Host:port for the bridge node; pending copy never includes a `{port}` literal. */
export function bridgeHostPortLabel(input: {
  host: string;
  port?: number | null;
}, t?: TranslateFn): string {
  const port = typeof input.port === 'number' && input.port > 0 ? input.port : null;
  if (port != null) return `${input.host}:${port}`;
  if (input.host === '127.0.0.1') {
    return detailText(t, 'routes.panel.bridge.hostPortPending', DETAIL_COPY.hostPortPending);
  }
  const pending = detailText(t, 'routes.panel.bridge.portPending', DETAIL_COPY.portPending);
  return withoutPendingPortLiteral(`${input.host} · ${pending}`, pending);
}

/** Bridge node helper: combine runtime + upstream into one line; port pending copy without `{port}` literal. */
export function bridgeNodeStatusLine(input: {
  runtimeLabel: string;
  upstreamLabel?: string | null;
  bridgeState?: string;
  statusUnavailable?: boolean;
}, t?: TranslateFn): { line: string; stoppedHint: string | null } {
  const pending = detailText(t, 'routes.panel.bridge.portPending', DETAIL_COPY.portPending);
  const runtime = withoutPendingPortLiteral(input.runtimeLabel, pending);
  const upstream = input.upstreamLabel
    ? withoutPendingPortLiteral(input.upstreamLabel, pending)
    : '';
  const line = input.statusUnavailable || !upstream.trim()
    ? runtime
    : `${runtime} · ${upstream}`;
  const stoppedHint = !input.statusUnavailable && input.bridgeState === 'stopped'
    ? detailText(t, 'routes.panel.bridge.stoppedHint', DETAIL_COPY.stoppedHint)
    : null;
  return { line, stoppedHint };
}

export function routeDetailTargetLabel(
  target: CreateRouteTarget,
  t?: TranslateFn,
): string {
  if (target === 'claude') return detailText(t, 'routes.create.target.claude', 'Claude');
  if (target === 'grok') return detailText(t, 'routes.create.target.grok', 'Grok');
  return detailText(t, 'routes.create.target.codex', 'Codex');
}
