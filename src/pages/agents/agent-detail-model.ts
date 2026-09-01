/**
 * Agent management detail view-model.
 *
 * Conversation surfaces are what this Agent speaks as a client — not dest
 * `RouteDownstreamSurface::for_agent` (that list is only local-route writers).
 * Shown on Agents detail. Path color reuses the Agent token
 * (`AGENT_COLORS` / `--agent-*`), not a second palette.
 */
import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import type { TranslateFn } from '@/lib/i18n';
import { isLiveFilePath, liveConfigPaths } from '@/lib/provider-detect';
import {
  localEndpointBrandAgentId,
  localEndpointKindForTargetAgent,
  routeEndpointBrandAgentId,
  routeEndpointPath,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import type { AgentStatus } from '@/lib/types';
import type { TokenAgentId } from '@/styles/tokens';
import { extraCopyKindLabel, extraCopyKindLabelKey, listAgentInstalls } from './agent-card-model';

export type AgentConversationSurface = RouteEndpointId;

export type AgentConversationEndpoint = {
  id: AgentConversationSurface;
  path: string;
  brandAgentId: TokenAgentId;
};

/**
 * HTTP conversation paths this Agent actually uses.
 *
 * Cursor Agent talks to Cursor's own backend (not these three paths) — omit.
 * Pi, ZCode, Kimi Code, and DeepSeek official API (DSH) can use all three.
 */
export function agentConversationSurfaces(
  agentId: string,
): readonly AgentConversationSurface[] {
  if (agentId === 'claude') return ['messages'];
  if (agentId === 'codex' || agentId === 'grok') return ['responses'];
  if (agentId === 'workbuddy') return ['chat_completions'];
  if (
    agentId === 'dsh'
    || agentId === 'pi'
    || agentId === 'zcode'
    || agentId === 'kimi'
  ) {
    return ['messages', 'responses', 'chat_completions'];
  }
  return [];
}

export function agentConversationSurface(
  agentId: string,
): AgentConversationSurface | null {
  return agentConversationSurfaces(agentId)[0] ?? null;
}

/** Paths this Agent speaks. Empty when the Agent has no public HTTP surface. */
export function agentConversationEndpoints(
  agentId: string,
): AgentConversationEndpoint[] {
  return agentConversationSurfaces(agentId).map((id) => ({
    id,
    path: routeEndpointPath(id),
    brandAgentId: id === 'responses' && agentId === 'grok'
      ? localEndpointBrandAgentId(localEndpointKindForTargetAgent(agentId))
      : routeEndpointBrandAgentId(id),
  }));
}

export function formatAgentConversationEndpoints(
  agentId: string,
  t: TranslateFn,
): string {
  const rows = agentConversationEndpoints(agentId);
  if (rows.length === 0) return t('agents.detail.endpointDependsOnLogin');
  return rows.map((row) => row.path).join('\n');
}

/** dest catalog prefixes the internal id (`native 官方脚本`); that is not product copy. */
export function isRawInstallChannelLabel(id: string, label: string): boolean {
  const trimmed = label.trim();
  if (!trimmed || trimmed === id) return true;
  if (id === 'native' && /^native(\s|$)/i.test(trimmed)) return true;
  return false;
}

/** Catalog `npm @scope/pkg` is a package id — fine on 安装位置, not as 渠道. */
export function isNpmPackageCatalogLabel(id: string, label: string): boolean {
  return id === 'npm' && /^npm\s+@/i.test(label.trim());
}

export function catalogChannelLabel(
  agentId: string,
  channel: string,
  catalogChannels?: readonly Pick<InstallChannelMeta, 'id' | 'label'>[],
): string | undefined {
  const channels = catalogChannels ?? AGENT_MAP[agentId]?.installChannels ?? [];
  const hit = channels.find((row) => row.id === channel);
  if (!hit?.label || isRawInstallChannelLabel(channel, hit.label)) return undefined;
  return hit.label.trim();
}

/**
 * 渠道: dest human kind only (官方脚本 / npm 包 / 官网 Setup / IDE / 桌面).
 * Package ids stay off this field.
 */
export function installChannelKindLabel(
  agentId: string,
  channel: string | null | undefined,
  t: TranslateFn,
  catalogChannels?: readonly Pick<InstallChannelMeta, 'id' | 'label'>[],
): string | undefined {
  const id = channel?.trim();
  if (!id) return undefined;
  if (id === 'npm') return extraCopyKindLabel('npm', t);
  const catalog = catalogChannelLabel(agentId, id, catalogChannels);
  if (catalog && !isNpmPackageCatalogLabel(id, catalog)) return catalog;
  if (extraCopyKindLabelKey(id)) return extraCopyKindLabel(id, t);
  return undefined;
}

/** 安装位置 may show the catalog package id; 渠道 must not. */
export function installLocationSourceLabel(
  agentId: string,
  channel: string | null | undefined,
  t: TranslateFn,
  catalogChannels?: readonly Pick<InstallChannelMeta, 'id' | 'label'>[],
): string | undefined {
  const id = channel?.trim();
  if (!id) return undefined;
  const catalog = catalogChannelLabel(agentId, id, catalogChannels);
  if (catalog) return catalog;
  return installChannelKindLabel(agentId, id, t, catalogChannels);
}

/** Catalog channels that are not on disk yet — list command + Install, never a path. */
export function missingCatalogChannels(
  agent: Pick<
    AgentStatus,
    'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies'
  >,
  catalogChannels?: readonly InstallChannelMeta[],
): InstallChannelMeta[] {
  const channels = catalogChannels ?? AGENT_MAP[agent.agentId]?.installChannels ?? [];
  const present = new Set<string>(listAgentInstalls(agent).map((row) => row.source));
  return channels.filter((channel) => channel.id.trim() && !present.has(channel.id));
}


export function isDisplayableConfigDir(value?: string | null): value is string {
  if (!value || !isLiveFilePath(value)) return false;
  const trimmed = value.trim();
  return trimmed !== '~' && trimmed !== '~/';
}

/** Prefer a resolved live dir; otherwise dest's known default, never a heading-only card. */
export function displayAgentConfigDir(
  agentId: string,
  resolvedOpenDir?: string | null,
): string | null {
  if (isDisplayableConfigDir(resolvedOpenDir)) return resolvedOpenDir.trim();
  const fallback = liveConfigPaths(agentId).openDir;
  const token = fallback.split(/[（(]/)[0]?.trim() ?? '';
  return isDisplayableConfigDir(token) ? token : null;
}
