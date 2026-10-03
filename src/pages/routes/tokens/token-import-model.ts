/**
 * Eligibility for one-click「导入到 Agent」from a local-route token.
 *
 * Confirm uses the Connections add-API-Key editor, then writes live config.
 * The menu follows who speaks the key's endpoint. Grok Responses is Grok-only.
 * Cursor has no public HTTP surface.
 */
import { isAgentHidden, visibleInstalledIds } from '@/lib/agent-visibility';
import {
  buildConnectionsGuideUrl,
  type ConnectApiKeyDraft,
  type PiProviderApi,
} from '@/lib/connect-flow/connect-intent';
import type { TranslateFn } from '@/lib/i18n';
import {
  localEndpointKindForTargetAgent,
  localEndpointSurface,
  ROUTE_ENDPOINT_HOST,
  routeEndpointHttpParts,
  type LocalEndpointKind,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { extractFormVars } from '@/lib/provider-detect/fields';
import type { AgentKey, AgentStatus, Provider } from '@/lib/types';
import { agentConversationSurfaces } from '@/pages/agents/agent-detail-model';
import { agentSupportsLocalEndpointKind, tokenListenPort, type LocalTokenRow } from './tokens-model';
import { hashLocalToken } from './token-connection-matches';

export type TokenImportAgentRef = {
  id: AgentKey;
  /** Display name for menus; callers may pass catalog name or id. */
  name: string;
};

export type TokenImportAgentChoice = TokenImportAgentRef & {
  enabled: boolean;
  /** Short per-row hint when this Agent cannot take the token. */
  reason: string | null;
};

/** Wire surface this token authenticates on. */
export function tokenImportSurface(kind: LocalEndpointKind): RouteEndpointId {
  return localEndpointSurface(kind);
}

/** True when the Agent speaks the token's conversation surface. */
export function agentMatchesTokenSurface(
  agentId: string,
  kind: LocalEndpointKind,
): boolean {
  const surface = tokenImportSurface(kind);
  return agentConversationSurfaces(agentId).includes(surface);
}

/**
 * Loopback writer kind for Agents that receive a generated local-gateway provider.
 * Null when bind/switch cannot write this token into the Agent.
 */
export function agentWritesLocalTokenKind(agentId: string): LocalEndpointKind | null {
  if (
    agentId === 'claude'
    || agentId === 'codex'
    || agentId === 'grok'
    || agentId === 'kimi'
    || agentId === 'dsh'
  ) {
    return localEndpointKindForTargetAgent(agentId);
  }
  return null;
}

/** True when this Agent speaks the key's endpoint and can take it as an API Key. */
export function agentCanReceiveTokenImport(
  agentId: string,
  kind: LocalEndpointKind,
): boolean {
  return agentSupportsLocalEndpointKind(agentId, kind);
}

/**
 * Installed, not hidden, and able to receive this token's loopback.
 * Order follows `installedIds` when provided (catalog / stored order).
 */
export function eligibleAgentsForTokenImport(input: {
  kind: LocalEndpointKind;
  /** Prefer explicit installed+visible ids from useInstalledAgents. */
  installedIds?: readonly string[];
  /** Fallback when only raw statuses are available. */
  statuses?: ReadonlyArray<Pick<AgentStatus, 'agentId' | 'installed' | 'hidden'>>;
  /** Optional name lookup; missing names fall back to id. */
  agentName?: (agentId: string) => string;
}): TokenImportAgentRef[] {
  const ids = input.installedIds
    ?? (input.statuses ? visibleInstalledIds(input.statuses) : []);
  const nameOf = input.agentName ?? ((id: string) => id);
  const out: TokenImportAgentRef[] = [];
  for (const id of ids) {
    if (!agentCanReceiveTokenImport(id, input.kind)) continue;
    out.push({ id: id as AgentKey, name: nameOf(id) || id });
  }
  return out;
}

/** Same visibility filter as visibleInstalledIds, for a single status row. */
export function isTokenImportAgentVisible(
  status: Pick<AgentStatus, 'installed' | 'hidden'> | null | undefined,
): boolean {
  return Boolean(status?.installed) && !isAgentHidden(status);
}

export function tokenImportAgentChoice(
  kind: LocalEndpointKind,
  agent: TokenImportAgentRef,
  t?: TranslateFn,
): TokenImportAgentChoice {
  if (agentCanReceiveTokenImport(agent.id, kind)) {
    return { ...agent, enabled: true, reason: null };
  }
  if (agentConversationSurfaces(agent.id).length === 0) {
    return {
      ...agent,
      enabled: false,
      reason: t ? t('routes.tokens.importCannotWrite') : '没有可用端点',
    };
  }
  return {
    ...agent,
    enabled: false,
    reason: t ? t('routes.tokens.importEndpointMismatch') : '端点不匹配',
  };
}

export type TokenImportGate = {
  enabled: boolean;
  /** Short hint when the control cannot open; null when the menu can open. */
  reason: string | null;
  agents: TokenImportAgentChoice[];
};

/**
 * Whether「导入到 Agent」can open a menu for this row.
 * The menu lists every installed Agent; items that cannot receive this token
 * stay visible and disabled with a short reason.
 */
export function tokenImportGate(
  row: Pick<LocalTokenRow, 'kind' | 'token' | 'unavailable'>,
  agents: readonly TokenImportAgentRef[],
  t?: TranslateFn,
): TokenImportGate {
  const choices = agents.map((agent) => tokenImportAgentChoice(row.kind, agent, t));
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
      agents: choices,
    };
  }
  if (!row.token?.trim()) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedKey') : '先有入口 Key 才能导入',
      agents: choices,
    };
  }
  if (agents.length === 0) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedAgent') : '先安装 Agent',
      agents: choices,
    };
  }
  return { enabled: true, reason: null, agents: choices };
}

export function tokenImportConnectionsUrl(agentId: AgentKey): string {
  return buildConnectionsGuideUrl({ agentId, intent: 'add-key' });
}

export function tokenImportApiKeyDraft(
  row: Pick<LocalTokenRow, 'kind' | 'token' | 'path' | 'endpoint' | 'listedModels'>,
  agentId: AgentKey,
): ConnectApiKeyDraft | null {
  const apiKey = row.token?.trim();
  if (!apiKey) return null;
  if (!agentCanReceiveTokenImport(agentId, row.kind)) return null;
  const parts = routeEndpointHttpParts({
    path: row.path,
    port: tokenListenPort(row.endpoint),
    host: ROUTE_ENDPOINT_HOST,
    endpointId: tokenImportSurface(row.kind),
  });
  const model = row.listedModels?.[0]?.trim() || '';
  const piApi: PiProviderApi | undefined = agentId === 'pi'
    ? row.kind === 'messages'
      ? 'anthropic-messages'
      : row.kind === 'responses_codex'
        ? 'openai-responses'
        : row.kind === 'chat_completions'
          ? 'openai-completions'
          : undefined
    : undefined;
  if (agentId === 'pi' && !piApi) return null;
  const baseUrl = parts.portPending
    ? undefined
    : agentId === 'pi' && row.kind === 'responses_codex'
      ? `${parts.origin}/v1`
      : parts.origin;
  return {
    ...(baseUrl ? { baseUrl } : {}),
    apiKey,
    ...(model ? { model } : {}),
    ...(piApi ? { piApi } : {}),
    ...(agentId === 'grok'
      ? { apiBackend: row.kind === 'chat_completions' ? 'chat_completions' : 'responses' }
      : {}),
  };
}

/** Compare imported Pi providers without merging a different URL, API, or model. */
export function canonicalTokenImportBaseUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed) return '';
  try {
    const parsed = new URL(trimmed);
    const path = parsed.pathname.replace(/\/+$/, '');
    return `${parsed.origin}${path}${parsed.search}`;
  } catch {
    return trimmed.replace(/\/+$/, '').toLowerCase();
  }
}

function providerUpdatedAt(provider: Provider): number {
  const parsed = Date.parse(provider.updatedAt ?? '');
  return Number.isNaN(parsed) ? 0 : parsed;
}

/** Find the newest exact Pi import match; no secret hash means no match. */
export async function findReusablePiTokenProvider(input: {
  draft: Pick<ConnectApiKeyDraft, 'baseUrl' | 'apiKey' | 'model' | 'piApi'>;
  providers: readonly Provider[];
}): Promise<Provider | null> {
  const apiKey = input.draft.apiKey?.trim() ?? '';
  const baseUrl = canonicalTokenImportBaseUrl(input.draft.baseUrl ?? '');
  const model = input.draft.model?.trim() ?? '';
  const piApi = input.draft.piApi?.trim() ?? '';
  if (!apiKey || !baseUrl || !model || !piApi) return null;

  const tokenHash = (await hashLocalToken(apiKey)).toLowerCase();
  if (!tokenHash) return null;
  const matches = input.providers.filter((provider) => {
    if (provider.agentId !== 'pi' || provider.home === 'route_pool') return false;
    if ((provider.secretHash?.trim().toLowerCase() ?? '') !== tokenHash) return false;
    const vars = extractFormVars('pi', provider.configText, provider.configFormat);
    return canonicalTokenImportBaseUrl(vars.baseUrl) === baseUrl
      && vars.piApi.trim() === piApi
      && vars.model.trim() === model;
  });
  return [...matches].sort((left, right) => (
    providerUpdatedAt(right) - providerUpdatedAt(left)
    || right.id.localeCompare(left.id)
  ))[0] ?? null;
}

/** Prefer the live profile object; fall back to the row's entry in sibling list. */
export function resolveTokenImportProfile(
  profile: AdapterProfile | null | undefined,
  profileId: string | null | undefined,
  siblings?: readonly AdapterProfile[],
): AdapterProfile | null {
  if (profile) return profile;
  const id = profileId?.trim();
  if (!id || !siblings?.length) return null;
  return siblings.find((item) => item.id === id) ?? null;
}
