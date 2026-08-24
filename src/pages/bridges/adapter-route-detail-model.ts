/**
 * Pure view-model for the route-detail relationship graph.
 * No React, no IO. Dialog UI stays on AdapterProfileDetailDialog.
 */
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { MessageKey, MessageParams, TranslateFn } from '@/lib/i18n';
import { ROUTE_ENDPOINT_PENDING_PORT, routeEndpointPath } from '@/lib/route-endpoints';
import type { AgentId } from '@/lib/types';
import {
  CREATE_ROUTE_TARGETS,
  listLocalRouteSurfacesFromConfig,
  readCreateRouteCapabilities,
  surfaceForCreateRouteTarget,
  type CreateRouteTarget,
} from './create-route-flow';
import { resolveAdapterProfileSource } from './adapter-view-model';

export type UpstreamChannel =
  | 'openai_chat'
  | 'anthropic_messages'
  | 'codex_responses'
  | 'grok_responses'
  | 'unknown';

export type RouteEdgeSupport =
  | 'source_missing'
  | 'hidden'
  | 'no_upstream'
  | 'applied'
  | 'ready'
  | 'runtime_only';

export type RouteDetailEdgeTarget = 'claude' | 'codex' | 'grok' | 'kimi' | 'dsh';

export type RouteHopKind = 'passthrough' | 'convert' | 'forward';

const PRODUCT_TARGETS: readonly CreateRouteTarget[] = CREATE_ROUTE_TARGETS;
const RUNTIME_ONLY_TARGETS = ['kimi', 'dsh'] as const;
const APPLIED_TARGETS = new Set<string>(['claude', 'codex', 'grok', 'kimi', 'dsh']);

const DETAIL_COPY = {
  sourceMissing: '来源登录已删除',
  hidden: '该客户端已在设置中隐藏',
  noUpstream: '来源未配置此客户端的上游端点',
  applied: '已写入 {name} 配置',
  ready: '可一键接入',
  runtimeOnly: '由后端路由支持，暂不提供界面配置',
  hopPassthrough: '直通上游',
  hopConvert: '转换 → 上游 {channel}',
  hopForward: '转发',
  channelOpenaiChat: 'Chat 接口',
  channelAnthropicMessages: 'Messages',
  channelCodexResponses: 'Codex Responses',
  channelGrokResponses: 'Grok Responses',
  channelUnknown: '未知',
  modelsOnly: '仅放行：{models}（其余模型将被拒绝）',
  modelsAny: '跟随客户端请求的模型',
  stoppedHint: '已停止——客户端暂时无法使用以下地址',
  portPending: '端口分配中',
  sourceDeletedHint: '来源登录已删除，路由仅可查看或解除绑定',
  applyConfirm: '将勾选项写入客户端配置',
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
  if (agent === 'claude') return 'anthropic_messages';
  if (agent === 'codex') return 'codex_responses';
  if (agent === 'grok') return 'grok_responses';
  if (input.mode === 'api' && (agent === 'kimi' || agent === 'dsh')) return 'openai_chat';
  return 'unknown';
}

function isEdgeTarget(value: string): value is RouteDetailEdgeTarget {
  return APPLIED_TARGETS.has(value);
}

/** Per-target applied set from sibling local_bridge profiles sharing sourceKind+sourceId with generatedProviderId. */
export function appliedTargetsFromProfiles(
  profiles: readonly Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>[],
  source: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
): ReadonlySet<RouteDetailEdgeTarget> {
  const applied = new Set<RouteDetailEdgeTarget>();
  for (const profile of profiles) {
    if (profile.route !== 'local_bridge') continue;
    if (profile.sourceKind !== source.sourceKind || profile.sourceId !== source.sourceId) continue;
    if (!profile.generatedProviderId?.trim()) continue;
    if (isEdgeTarget(profile.targetAgentId)) applied.add(profile.targetAgentId);
  }
  return applied;
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
  sourceAgentId: AgentId | null;
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
  agentId: AgentId | null;
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

export type RouteDetailEdgeView = {
  target: RouteDetailEdgeTarget;
  endpointId: 'messages' | 'responses' | 'chat_completions';
  path: string;
  support: RouteEdgeSupport;
  hop: RouteHopKind;
  /** Upstream channel used for hop labeling; unknown → hop forward */
  upstreamChannel: UpstreamChannel;
  /** Per-target upstream URL if different from base / useful on the edge */
  upstreamUrl: string;
  selectable: boolean;
};

function hopFor(
  endpointId: RouteDetailEdgeView['endpointId'],
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

function resolveProductSupport(input: {
  missing: boolean;
  hidden: boolean;
  noUpstream: boolean;
  applied: boolean;
}): RouteEdgeSupport {
  if (input.missing) return 'source_missing';
  if (input.hidden) return 'hidden';
  if (input.noUpstream) return 'no_upstream';
  if (input.applied) return 'applied';
  return 'ready';
}

function resolveRuntimeOnlySupport(input: {
  missing: boolean;
  hidden: boolean;
}): RouteEdgeSupport {
  if (input.missing) return 'source_missing';
  if (input.hidden) return 'hidden';
  return 'runtime_only';
}

function edgeView(input: {
  target: RouteDetailEdgeTarget;
  endpointId: RouteDetailEdgeView['endpointId'];
  path: string;
  support: RouteEdgeSupport;
  channel: UpstreamChannel;
  upstreamUrl: string;
}): RouteDetailEdgeView {
  return {
    target: input.target,
    endpointId: input.endpointId,
    path: input.path,
    support: input.support,
    hop: hopFor(input.endpointId, input.channel),
    upstreamChannel: input.channel,
    upstreamUrl: input.upstreamUrl,
    selectable: input.support === 'ready' || input.support === 'applied',
  };
}

function productTargetsToEmit(
  hasDeclaredEndpoints: boolean,
  surfaces: readonly { target: CreateRouteTarget }[],
): readonly CreateRouteTarget[] {
  if (hasDeclaredEndpoints) return PRODUCT_TARGETS;
  const seen = new Set<CreateRouteTarget>();
  const targets: CreateRouteTarget[] = [];
  for (const surface of surfaces) {
    if (seen.has(surface.target)) continue;
    seen.add(surface.target);
    targets.push(surface.target);
  }
  return targets;
}

function perTargetUrl(
  endpoints: readonly { target: CreateRouteTarget; url: string }[],
  target: CreateRouteTarget,
  baseUrl: string,
): string {
  const url = endpoints.find((row) => row.target === target)?.url.trim() ?? '';
  if (!url) return '';
  return url === baseUrl.trim() ? '' : url;
}

function channelForEdge(input: {
  missing: boolean;
  perTargetUrl: string;
  sourceChannel: UpstreamChannel;
}): UpstreamChannel {
  if (input.missing) return 'unknown';
  if (isUsableUrl(input.perTargetUrl)) return detectUpstreamChannelFromUrl(input.perTargetUrl);
  return input.sourceChannel;
}

/**
 * Build product edges (claude/codex/grok from surfaces) + runtime_only kimi/dsh only when applied sibling exists.
 * Priority for support: source_missing > hidden > no_upstream > applied > ready.
 * no_upstream only when capabilities.endpoints is non-empty AND target missing from enabled endpoints.
 * When capabilities.endpoints empty, do NOT emit no_upstream (follow surfaces fallback).
 * hiddenTargetIds: check edge target id as string.
 */
export function buildRouteDetailEdges(input: {
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name' | 'mode' | 'targetAgentId' | 'ruleId' | 'route'>;
  entries: readonly ConnectionEntry[];
  siblingProfiles: readonly Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>[];
  hiddenTargetIds?: ReadonlySet<string>;
}): RouteDetailEdgeView[] {
  const source = buildRouteDetailSourceView(input);
  const entry = matchSourceEntry(input.profile, input.entries);
  const configText = source.missing ? undefined : entry?.provider?.configText;
  const caps = readCreateRouteCapabilities(configText);
  const surfaces = listLocalRouteSurfacesFromConfig(configText, {
    targetAgentId: input.profile.targetAgentId,
    ruleId: input.profile.ruleId,
  });
  const applied = appliedTargetsFromProfiles(input.siblingProfiles, input.profile);
  const hiddenTargetIds = input.hiddenTargetIds ?? new Set<string>();
  const enabledTargets = new Set(caps.endpoints.map((row) => row.target));
  const hasDeclaredEndpoints = caps.endpoints.length > 0;
  const edges: RouteDetailEdgeView[] = [];

  for (const target of productTargetsToEmit(hasDeclaredEndpoints, surfaces)) {
    const surface = surfaces.find((row) => row.target === target) ?? surfaceForCreateRouteTarget(target);
    const targetUrl = perTargetUrl(caps.endpoints, target, source.baseUrl);
    const channel = channelForEdge({
      missing: source.missing,
      perTargetUrl: targetUrl || caps.endpoints.find((row) => row.target === target)?.url || '',
      sourceChannel: source.channel,
    });
    const support = resolveProductSupport({
      missing: source.missing,
      hidden: hiddenTargetIds.has(target),
      noUpstream: hasDeclaredEndpoints && !enabledTargets.has(target),
      applied: applied.has(target),
    });
    edges.push(edgeView({
      target,
      endpointId: surface.endpointId,
      path: surface.path,
      support,
      channel,
      upstreamUrl: targetUrl,
    }));
  }

  for (const target of RUNTIME_ONLY_TARGETS) {
    if (!applied.has(target)) continue;
    const support = resolveRuntimeOnlySupport({
      missing: source.missing,
      hidden: hiddenTargetIds.has(target),
    });
    edges.push(edgeView({
      target,
      endpointId: 'chat_completions',
      path: routeEndpointPath('chat_completions'),
      support,
      channel: channelForEdge({
        missing: source.missing,
        perTargetUrl: '',
        sourceChannel: source.channel,
      }),
      upstreamUrl: '',
    }));
  }

  return edges;
}

export function routeEdgeSupportLabel(
  support: RouteEdgeSupport,
  targetLabel: string,
  t?: TranslateFn,
): string {
  if (support === 'source_missing') {
    return detailText(t, 'routes.detail.edge.sourceMissing', DETAIL_COPY.sourceMissing);
  }
  if (support === 'hidden') {
    return detailText(t, 'routes.detail.edge.hidden', DETAIL_COPY.hidden);
  }
  if (support === 'no_upstream') {
    return detailText(t, 'routes.detail.edge.noUpstream', DETAIL_COPY.noUpstream);
  }
  if (support === 'applied') {
    return detailText(t, 'routes.detail.edge.applied', DETAIL_COPY.applied, { name: targetLabel });
  }
  if (support === 'runtime_only') {
    return detailText(t, 'routes.detail.edge.runtimeOnly', DETAIL_COPY.runtimeOnly);
  }
  return detailText(t, 'routes.detail.edge.ready', DETAIL_COPY.ready);
}

export function upstreamChannelLabel(channel: UpstreamChannel, t?: TranslateFn): string {
  if (channel === 'openai_chat') {
    return detailText(t, 'routes.detail.channel.openaiChat', DETAIL_COPY.channelOpenaiChat);
  }
  if (channel === 'anthropic_messages') {
    return detailText(t, 'routes.detail.channel.anthropicMessages', DETAIL_COPY.channelAnthropicMessages);
  }
  if (channel === 'codex_responses') {
    return detailText(t, 'routes.detail.channel.codexResponses', DETAIL_COPY.channelCodexResponses);
  }
  if (channel === 'grok_responses') {
    return detailText(t, 'routes.detail.channel.grokResponses', DETAIL_COPY.channelGrokResponses);
  }
  return detailText(t, 'routes.detail.channel.unknown', DETAIL_COPY.channelUnknown);
}

export function routeHopLabel(hop: RouteHopKind, channel: UpstreamChannel, t?: TranslateFn): string {
  if (hop === 'passthrough') {
    return detailText(t, 'routes.detail.hop.passthrough', DETAIL_COPY.hopPassthrough);
  }
  if (hop === 'convert') {
    return detailText(t, 'routes.detail.hop.convert', DETAIL_COPY.hopConvert, {
      channel: upstreamChannelLabel(channel, t),
    });
  }
  return detailText(t, 'routes.detail.hop.forward', DETAIL_COPY.hopForward);
}

export function routeModelsSummary(models: readonly string[], t?: TranslateFn): string {
  if (models.length === 0) {
    return detailText(t, 'routes.detail.models.any', DETAIL_COPY.modelsAny);
  }
  return detailText(t, 'routes.detail.models.only', DETAIL_COPY.modelsOnly, {
    models: models.join(', '),
  });
}

export function routeSourceDeletedHint(t?: TranslateFn): string {
  return detailText(t, 'routes.detail.source.deletedHint', DETAIL_COPY.sourceDeletedHint);
}

export function routeDetailApplyConfirmLabel(t?: TranslateFn): string {
  return detailText(t, 'routes.detail.apply.confirm', DETAIL_COPY.applyConfirm);
}

export function routeCopyPortPendingLabel(t?: TranslateFn): string {
  return detailText(t, 'routes.detail.copyPortPending', DETAIL_COPY.copyPortPending);
}

/** Bridge node helper: combine runtime + upstream into one line; port pending copy without `{port}` literal. */
export function bridgeNodeStatusLine(input: {
  runtimeLabel: string;
  upstreamLabel?: string | null;
  bridgeState?: string;
  statusUnavailable?: boolean;
}, t?: TranslateFn): { line: string; stoppedHint: string | null } {
  const pending = detailText(t, 'routes.detail.bridge.portPending', DETAIL_COPY.portPending);
  const runtime = withoutPendingPortLiteral(input.runtimeLabel, pending);
  const upstream = input.upstreamLabel
    ? withoutPendingPortLiteral(input.upstreamLabel, pending)
    : '';
  const line = input.statusUnavailable || !upstream.trim()
    ? runtime
    : `${runtime} · ${upstream}`;
  const stoppedHint = !input.statusUnavailable && input.bridgeState === 'stopped'
    ? detailText(t, 'routes.detail.bridge.stoppedHint', DETAIL_COPY.stoppedHint)
    : null;
  return { line, stoppedHint };
}
