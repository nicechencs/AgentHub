import { deleteProvider, upsertProvider } from '@/lib/api/provider';
import { bindTicket, planTicket, ticketIdFor, unbindTicket } from '@/lib/api/tickets';
import {
  isInternalGeneratedProvider,
  isLoopbackUrl,
} from '@/lib/backend/contracts/agent-connection';
import type { RouteEndpointId } from '@/lib/route-endpoints';
import { routeEndpointIdForBinding, routeEndpointPath } from '@/lib/route-endpoints';
import type { AgentId, AppSettings, Provider } from '@/lib/types';
import {
  contextWindowTokensFromChoice,
  parseContextWindowChoice,
  stripClaudeContextMarker,
  type ClaudeContextWindowChoice,
} from '@/lib/claude-client-env';
import { isLeftoverLocalRouteProvider } from '@/lib/leftover-local-route';
import { detectUpstreamChannelFromUrl } from './adapter-route-detail-model';

export const CREATE_ROUTE_TARGETS = ['claude', 'codex', 'grok'] as const;
/** OpenRouter no longer pins a stealth backup; listed models come from the login. */
export const DEFAULT_CREATE_ROUTE_MODEL = '';
export const DEFAULT_CREATE_ROUTE_URL = 'https://openrouter.ai/api/v1';

export type CreateRouteTarget = (typeof CREATE_ROUTE_TARGETS)[number];

export type CreateRouteVendorId =
  | 'openrouter'
  | 'openai'
  | 'glm'
  | 'kimi'
  | 'deepseek'
  | 'grok'
  | 'claude'
  | 'custom';

export type CreateRouteVendor = {
  id: CreateRouteVendorId;
  url: string;
  enabled: readonly CreateRouteTarget[];
  models: readonly string[];
  /** Stored on the route; OpenRouter defaults to 1M so Claude Code compact matches typical OR models. */
  defaultContextWindow?: ClaudeContextWindowChoice;
  endpointUrls?: Partial<Record<CreateRouteTarget, string>>;
};

export const CREATE_ROUTE_VENDORS: readonly CreateRouteVendor[] = [
  {
    id: 'openrouter',
    url: DEFAULT_CREATE_ROUTE_URL,
    enabled: ['claude', 'codex', 'grok'],
    models: [],
    defaultContextWindow: '1048576',
  },
  {
    id: 'openai',
    url: 'https://api.openai.com/v1',
    enabled: ['codex', 'grok'],
    models: [],
  },
  {
    id: 'glm',
    url: 'https://open.bigmodel.cn/api/coding/paas/v4',
    enabled: ['claude', 'codex', 'grok'],
    models: [],
    endpointUrls: {
      claude: 'https://open.bigmodel.cn/api/anthropic',
      codex: 'https://open.bigmodel.cn/api/coding/paas/v4',
      grok: 'https://open.bigmodel.cn/api/coding/paas/v4',
    },
  },
  {
    id: 'kimi',
    url: 'https://api.moonshot.cn/v1',
    enabled: ['codex', 'grok'],
    models: [],
  },
  {
    id: 'deepseek',
    url: 'https://api.deepseek.com',
    enabled: ['claude', 'codex'],
    models: [],
    endpointUrls: {
      claude: 'https://api.deepseek.com/anthropic',
    },
  },
  {
    id: 'grok',
    url: 'https://api.x.ai/v1',
    enabled: ['codex', 'grok'],
    models: [],
  },
  {
    id: 'claude',
    url: 'https://api.anthropic.com',
    enabled: ['claude'],
    models: [],
  },
  {
    id: 'custom',
    url: '',
    enabled: [],
    models: [],
  },
];

export type CreateRouteEndpoint = {
  target: CreateRouteTarget;
  enabled: boolean;
  url: string;
};

export type CreateRouteInput = {
  name: string;
  url: string;
  key: string;
  vendor: CreateRouteVendorId;
  endpoints: readonly CreateRouteTarget[];
  models?: string;
  /** `auto` omits window env; otherwise a token count written to the client. */
  contextWindow?: ClaudeContextWindowChoice;
  /** Per-client upstream base URL overrides (custom vendor). */
  endpointUrls?: Partial<Record<CreateRouteTarget, string>>;
};

export type ImportRouteInput = {
  sourceKind: 'account' | 'provider';
  sourceId: string;
  agentId: AgentId;
};

export type CreateRouteDeps = {
  upsertProvider: typeof upsertProvider;
  planTicket: typeof planTicket;
  bindTicket: typeof bindTicket;
  unbindTicket: typeof unbindTicket;
  deleteProvider: typeof deleteProvider;
};

const defaultDeps: CreateRouteDeps = {
  upsertProvider,
  planTicket,
  bindTicket,
  unbindTicket,
  deleteProvider,
};

export function vendorById(id: CreateRouteVendorId): CreateRouteVendor {
  return CREATE_ROUTE_VENDORS.find((vendor) => vendor.id === id) ?? CREATE_ROUTE_VENDORS[0]!;
}

export function normalizeCreateRouteUrl(url: string): string {
  return url.trim().replace(/\/+$/, '');
}

export function isOpenRouterUrl(url: string): boolean {
  return normalizeCreateRouteUrl(url).toLowerCase().includes('openrouter.ai');
}

export function isCreateRouteUrlValid(url: string): boolean {
  const normalized = url.trim().toLowerCase();
  return normalized.startsWith('http://') || normalized.startsWith('https://');
}

export function parseCreateRouteModels(text: string | undefined): string[] {
  if (!text) return [];
  const models: string[] = [];
  for (const item of text.split(/[,，\n]+/)) {
    const id = stripClaudeContextMarker(item);
    if (!id) continue;
    if (models.some((existing) => existing.toLowerCase() === id.toLowerCase())) continue;
    models.push(id);
  }
  return models;
}

export function formatCreateRouteModels(models: readonly string[]): string {
  return models.join(', ');
}

export function defaultCreateRouteEndpoints(vendor: CreateRouteVendorId): CreateRouteTarget[] {
  return [...vendorById(vendor).enabled];
}

export function defaultCreateRouteName(vendorLabel: string): string {
  return vendorLabel.trim().replace(/\s+/g, ' ');
}

export function createRouteAutoNames(vendorLabels: readonly string[]): string[] {
  return vendorLabels.map((label) => defaultCreateRouteName(label));
}

export function isAutoCreateRouteName(name: string, autoNames: readonly string[]): boolean {
  const trimmed = name.trim();
  if (!trimmed) return true;
  return autoNames.some((item) => item.trim() === trimmed);
}

export function nextCreateRouteName(
  current: string,
  nextAuto: string,
  autoNames: readonly string[],
): string {
  return isAutoCreateRouteName(current, autoNames) ? nextAuto : current;
}

export function importRouteRowTitle(
  entry: {
    title: string;
    subtitle: string;
    agentId: string;
    source: 'account' | 'provider';
    endpointMode?: 'official' | 'custom';
  },
  labels: { agent: string; officialEndpoint: string; customEndpoint: string },
): string {
  const bits = [entry.title.trim(), labels.agent.trim()];
  if (entry.endpointMode === 'custom') bits.push(labels.customEndpoint.trim());
  else if (entry.endpointMode === 'official') bits.push(labels.officialEndpoint.trim());
  return bits.filter(Boolean).join(' · ');
}

export function connectionSourceKey(
  source: 'account' | 'provider',
  id: string,
): string {
  return `${source}:${id}`;
}

export type RoutedProfileHint = {
  id: string;
  name?: string;
  sourceKind: 'account' | 'provider';
  sourceId: string;
  route: string;
  generatedProviderId?: string | null;
};

function unwrapTicketId(source: 'account' | 'provider', id: string): string {
  const prefix = `${source}:`;
  return id.startsWith(prefix) ? id.slice(prefix.length) : id;
}

function occupySourceId(keys: Set<string>, source: 'account' | 'provider', raw: string): void {
  const id = raw.trim();
  if (!id) return;
  const bare = unwrapTicketId(source, id);
  keys.add(connectionSourceKey(source, id));
  keys.add(connectionSourceKey(source, bare));
}

const RELATED_ID_MIN = 16;

/** Prefix, suffix, or ticket-wrapped ids that refer to the same login. */
export function sourceIdsRelated(left: string, right: string): boolean {
  const a = left.trim();
  const b = right.trim();
  if (!a || !b) return false;
  if (a === b) return true;
  const aBare = a.includes(':') ? a.slice(a.indexOf(':') + 1) : a;
  const bBare = b.includes(':') ? b.slice(b.indexOf(':') + 1) : b;
  if (aBare && aBare === bBare) return true;
  if (a.length >= RELATED_ID_MIN && b.length >= RELATED_ID_MIN) {
    if (a.startsWith(b) || b.startsWith(a) || a.endsWith(b) || b.endsWith(a)) return true;
    if (aBare.startsWith(bBare) || bBare.startsWith(aBare) || aBare.endsWith(bBare) || bBare.endsWith(aBare)) {
      return aBare.length >= RELATED_ID_MIN && bBare.length >= RELATED_ID_MIN;
    }
  }
  return false;
}

function idEmbedded(needle: string, haystack: string | null | undefined): boolean {
  const id = needle.trim();
  const field = haystack?.trim() ?? '';
  if (!id || !field || id.length < RELATED_ID_MIN) return false;
  return field === id || field.includes(id) || id.includes(field);
}

/** Any AdapterProfile occupies its source and generated provider login. */
export function alreadyRoutedSourceKeys(
  profiles: readonly RoutedProfileHint[],
  _bindingProfileIds?: ReadonlySet<string>,
): Set<string> {
  const keys = new Set<string>();
  for (const profile of profiles) {
    occupySourceId(keys, profile.sourceKind, profile.sourceId);
    const generated = profile.generatedProviderId?.trim();
    if (generated) occupySourceId(keys, 'provider', generated);
  }
  return keys;
}

function isLoopbackHost(host: string | undefined): boolean {
  const value = host?.trim() ?? '';
  if (!value) return false;
  if (isLoopbackUrl(value) || isLoopbackUrl(`http://${value}`)) return true;
  return /127\.0\.0\.1|localhost|\[?::1\]?/i.test(value);
}

export function isGeneratedLocalRouteEntry(entry: {
  title?: string;
  endpointHost?: string;
  provider?: Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>;
}): boolean {
  if ((entry.title ?? '').includes('本机路由')) return true;
  if (isLoopbackHost(entry.endpointHost)) return true;
  if (!entry.provider) return false;
  return isLeftoverLocalRouteProvider(entry.provider)
    || isInternalGeneratedProvider(entry.provider);
}

/** Trim + strip trailing slash + optional trailing `/v1` for same-route URL compare. */
export function normalizeRouteCompareUrl(url: string): string {
  const trimmed = normalizeCreateRouteUrl(url);
  if (!trimmed) return '';
  return trimmed
    .replace(/\/v1$/i, '')
    .replace(/\/+$/, '');
}

export function readRouteProviderBaseUrl(configText: string | undefined): string {
  return readCreateRouteConfigMeta(configText).baseUrl;
}

/** SHA-256 hex of a trimmed secret (matches core `secret_sha256_hex`). */
export async function sha256Hex(secret: string): Promise<string> {
  const data = new TextEncoder().encode(secret.trim());
  const digest = await crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

export type RouteDuplicatePolicy = {
  /** Tip when key/login already used. Default true. Does not block. */
  warnDuplicateCredential: boolean;
  /** Same Agent + same URL → update existing route. Default true. */
  updateDuplicateUrl: boolean;
};

export const DEFAULT_ROUTE_DUPLICATE_POLICY: RouteDuplicatePolicy = {
  warnDuplicateCredential: true,
  updateDuplicateUrl: true,
};

export function routeDuplicatePolicyFromSettings(
  settings: Partial<Pick<AppSettings, 'warnDuplicateRouteCredential' | 'updateDuplicateRouteUrl'>> | null | undefined,
): RouteDuplicatePolicy {
  return {
    warnDuplicateCredential: settings?.warnDuplicateRouteCredential !== false,
    updateDuplicateUrl: settings?.updateDuplicateRouteUrl !== false,
  };
}

/** Existing user route (not a generated local leftover) with the same Agent + URL. */
export function findRouteProviderByUrl(
  providers: readonly Provider[],
  url: string,
  agentId: AgentId,
): Provider | undefined {
  const target = normalizeRouteCompareUrl(url);
  if (!target) return undefined;
  return providers.find((provider) => {
    if (provider.agentId !== agentId) return false;
    if (isGeneratedLocalRouteEntry({ title: provider.name, provider })) return false;
    return normalizeRouteCompareUrl(readRouteProviderBaseUrl(provider.configText)) === target;
  });
}

export async function routeCredentialMatchesExisting(
  key: string,
  providers: readonly Provider[],
  accounts: readonly { secretHash?: string | null }[] = [],
): Promise<boolean> {
  const secret = key.trim();
  if (!secret) return false;
  const hash = await sha256Hex(secret);
  if (accounts.some((account) => (account.secretHash?.trim() ?? '') === hash)) {
    return true;
  }
  return providers.some((provider) => {
    if (isGeneratedLocalRouteEntry({ title: provider.name, provider })) return false;
    return (provider.secretHash?.trim() ?? '') === hash;
  });
}

export type CreateRouteSubmitResult = {
  agents: string[];
  updatedExisting: boolean;
};

export type CreateRouteSubmitContext = {
  existingProviders?: readonly Provider[];
  policy?: Partial<RouteDuplicatePolicy>;
};

function entryMatchesRoutedProfile<T extends {
  source: 'account' | 'provider';
  id: string;
  title?: string;
  provider?: Pick<Provider, 'id' | 'name'>;
}>(entry: T, profile: RoutedProfileHint): boolean {
  const title = entry.title?.trim();
  if (title && profile.name?.trim() === title) return true;
  if (sourceIdsRelated(entry.id, profile.sourceId)) return true;
  if (profile.generatedProviderId && sourceIdsRelated(entry.id, profile.generatedProviderId)) return true;
  const providerId = entry.provider?.id?.trim();
  if (providerId) {
    if (idEmbedded(providerId, profile.sourceId) || idEmbedded(providerId, profile.generatedProviderId)) {
      return true;
    }
    if (sourceIdsRelated(providerId, profile.sourceId)) return true;
    if (profile.generatedProviderId && sourceIdsRelated(providerId, profile.generatedProviderId)) return true;
  }
  return false;
}

export type ImportableConnectionOptions = {
  /**
   * When true (default), already-routed logins stay listed and are flagged for a tip.
   * When false, the duplicate check is off: still list them, but do not flag.
   * Generated local leftovers stay hidden either way.
   */
  checkDuplicateCredential?: boolean;
};

export type ImportableConnectionEntry<T> = T & { alreadyRouted?: boolean };

function entryIsAlreadyRouted<T extends {
  source: 'account' | 'provider';
  id: string;
  title?: string;
  provider?: Pick<Provider, 'id' | 'name'>;
}>(
  entry: T,
  routedKeys: ReadonlySet<string>,
  profiles: readonly RoutedProfileHint[],
): boolean {
  if (routedKeys.has(connectionSourceKey(entry.source, entry.id))) return true;
  return profiles.some((profile) => entryMatchesRoutedProfile(entry, profile));
}

export function importableConnectionEntries<T extends {
  source: 'account' | 'provider';
  id: string;
  title?: string;
  endpointHost?: string;
  provider?: Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>;
}>(
  entries: readonly T[],
  routedKeys: ReadonlySet<string>,
  profiles: readonly RoutedProfileHint[] = [],
  options: ImportableConnectionOptions = {},
): ImportableConnectionEntry<T>[] {
  const checkDuplicate = options.checkDuplicateCredential !== false;
  return entries.flatMap((entry) => {
    if (isGeneratedLocalRouteEntry(entry)) return [];
    const alreadyRouted = entryIsAlreadyRouted(entry, routedKeys, profiles);
    return [{
      ...entry,
      ...(checkDuplicate && alreadyRouted ? { alreadyRouted: true as const } : {}),
    }];
  });
}

export function endpointUrlFor(
  vendor: CreateRouteVendorId,
  target: CreateRouteTarget,
  formUrl: string,
  overrides?: Partial<Record<CreateRouteTarget, string>>,
): string {
  const override = overrides?.[target]?.trim();
  if (override) return override;
  const spec = vendorById(vendor);
  const primary = normalizeCreateRouteUrl(spec.url);
  const current = normalizeCreateRouteUrl(formUrl);
  const specific = spec.endpointUrls?.[target];
  if (specific && (!current || current === primary || current === normalizeCreateRouteUrl(specific))) {
    return specific;
  }
  return formUrl.trim();
}

/** Upstream API path (e.g. /v1/chat/completions) inferred from a provider base URL. */
export function upstreamEndpointPathForUrl(url: string): string {
  const channel = detectUpstreamChannelFromUrl(url);
  if (channel === 'anthropic_messages') return '/v1/messages';
  if (channel === 'codex_responses' || channel === 'grok_responses') return '/v1/responses';
  if (channel === 'openai_chat') return '/v1/chat/completions';
  return '';
}

export function upstreamEndpointPathForTarget(
  vendor: CreateRouteVendorId,
  target: CreateRouteTarget,
  formUrl: string,
  overrides?: Partial<Record<CreateRouteTarget, string>>,
): string {
  return upstreamEndpointPathForUrl(endpointUrlFor(vendor, target, formUrl, overrides));
}

export function buildCreateRouteEndpoints(
  vendor: CreateRouteVendorId,
  formUrl: string,
  enabled: readonly CreateRouteTarget[],
  overrides?: Partial<Record<CreateRouteTarget, string>>,
): CreateRouteEndpoint[] {
  return CREATE_ROUTE_TARGETS.map((target) => ({
    target,
    enabled: enabled.includes(target),
    url: endpointUrlFor(vendor, target, formUrl, overrides),
  }));
}

export function canSubmitCreateRoute(input: CreateRouteInput): boolean {
  return Boolean(
    input.name.trim()
    && input.key.trim()
    && isCreateRouteUrlValid(input.url)
    && input.endpoints.length > 0,
  );
}

export function createRouteOwner(endpoints: readonly CreateRouteTarget[]): CreateRouteTarget {
  return CREATE_ROUTE_TARGETS.find((target) => endpoints.includes(target)) ?? 'codex';
}

export function createRouteProviderDraft(input: CreateRouteInput): Provider {
  const url = normalizeCreateRouteUrl(input.url);
  const key = input.key.trim();
  const models = parseCreateRouteModels(input.models);
  const endpoints = buildCreateRouteEndpoints(input.vendor, url, input.endpoints, input.endpointUrls)
    .filter((row) => row.enabled);
  const settings: Record<string, unknown> = {
    baseURL: url,
    apiKey: key,
    vendor: input.vendor,
    endpoints,
    listedModels: models,
  };
  if (models[0]) settings.model = models[0];
  const windowTokens = contextWindowTokensFromChoice(input.contextWindow ?? 'auto');
  if (windowTokens) settings.contextWindowTokens = windowTokens;
  else delete settings.contextWindowTokens;
  return {
    id: `openai-compat-${crypto.randomUUID()}`,
    agentId: createRouteOwner(input.endpoints),
    name: input.name.trim(),
    preset: isOpenRouterUrl(url) || input.vendor === 'openrouter' ? 'openrouter' : 'openai-compat',
    configText: JSON.stringify(settings, null, 2),
    configFormat: 'json',
    isCurrent: false,
    official: false,
  };
}

export function readCreateRouteCapabilities(configText: string | undefined): {
  endpoints: CreateRouteEndpoint[];
  models: string[];
  contextWindow: ClaudeContextWindowChoice;
} {
  try {
    const parsed = JSON.parse(configText ?? '{}') as {
      listedModels?: unknown;
      model?: unknown;
      endpoints?: unknown;
      contextWindowTokens?: unknown;
    };
    const models = Array.isArray(parsed.listedModels)
      ? parseCreateRouteModels(
          parsed.listedModels
            .filter((item): item is string => typeof item === 'string')
            .join(','),
        )
      : parseCreateRouteModels(typeof parsed.model === 'string' ? parsed.model : '');
    const contextWindow = parseContextWindowChoice(
      typeof parsed.contextWindowTokens === 'number'
        ? String(parsed.contextWindowTokens)
        : typeof parsed.contextWindowTokens === 'string'
          ? parsed.contextWindowTokens
          : '',
    );
    const endpoints = Array.isArray(parsed.endpoints)
      ? parsed.endpoints.flatMap((row) => {
          if (!row || typeof row !== 'object') return [];
          const record = row as { target?: unknown; enabled?: unknown; url?: unknown };
          if (record.target !== 'claude' && record.target !== 'codex' && record.target !== 'grok') {
            return [];
          }
          const target: CreateRouteTarget = record.target;
          return [{
            target,
            enabled: record.enabled !== false,
            url: typeof record.url === 'string' ? record.url : '',
          }];
        })
      : [];
    return { endpoints: endpoints.filter((row) => row.enabled), models, contextWindow };
  } catch {
    return { endpoints: [], models: [], contextWindow: 'auto' };
  }
}


export type LocalRouteSurface = {
  target: CreateRouteTarget;
  endpointId: RouteEndpointId;
  path: string;
};

export function surfaceForCreateRouteTarget(target: CreateRouteTarget): LocalRouteSurface {
  if (target === 'claude') {
    return { target, endpointId: 'messages', path: '/v1/messages' };
  }
  return { target, endpointId: 'responses', path: '/v1/responses' };
}

function readCreateRouteConfigMeta(configText: string | undefined): {
  vendor: string | null;
  baseUrl: string;
  hasEndpointsField: boolean;
} {
  try {
    const parsed = JSON.parse(configText ?? '{}') as {
      vendor?: unknown;
      baseURL?: unknown;
      baseUrl?: unknown;
      base_url?: unknown;
      endpoints?: unknown;
    };
    const vendor = typeof parsed.vendor === 'string' ? parsed.vendor : null;
    const candidates = [parsed.baseURL, parsed.baseUrl, parsed.base_url];
    const baseUrl = candidates
      .map((value) => (typeof value === 'string' ? value.trim() : ''))
      .find((value) => value.length > 0) ?? '';
    return {
      vendor,
      baseUrl,
      hasEndpointsField: Array.isArray(parsed.endpoints),
    };
  } catch {
    return { vendor: null, baseUrl: '', hasEndpointsField: false };
  }
}

/** Local client surfaces for one route row (same port). */
export function listLocalRouteSurfacesFromConfig(
  configText: string | undefined,
  fallback: { targetAgentId: AgentId | string; ruleId?: string | null },
): LocalRouteSurface[] {
  const caps = readCreateRouteCapabilities(configText);
  if (caps.endpoints.length > 0) {
    return caps.endpoints.map((row) => surfaceForCreateRouteTarget(row.target));
  }
  const meta = readCreateRouteConfigMeta(configText);
  if (meta.vendor === 'openrouter' || isOpenRouterUrl(meta.baseUrl) || meta.hasEndpointsField) {
    return CREATE_ROUTE_TARGETS.map((target) => surfaceForCreateRouteTarget(target));
  }
  const endpointId = routeEndpointIdForBinding({
    agentId: fallback.targetAgentId,
    ruleId: fallback.ruleId,
  });
  const target: CreateRouteTarget = fallback.targetAgentId === 'claude'
    ? 'claude'
    : fallback.targetAgentId === 'grok'
      ? 'grok'
      : 'codex';
  return [{ target, endpointId, path: routeEndpointPath(endpointId) }];
}

/**
 * A failed 确认应用 / unbind / stop must not look like success.
 * Compensation failure wins so the swallowed rollback is visible.
 */
export function surfaceAfterCompensation(
  original: unknown,
  compensationFailures: readonly unknown[],
): unknown {
  return compensationFailures[0] ?? original;
}

export async function applyLocalRouteToAgents(
  input: {
    sourceKind: 'account' | 'provider';
    sourceId: string;
    agents: readonly CreateRouteTarget[];
  },
  deps: Pick<CreateRouteDeps, 'planTicket' | 'bindTicket' | 'unbindTicket'> = defaultDeps,
): Promise<CreateRouteTarget[]> {
  const selected = CREATE_ROUTE_TARGETS.filter((target) => input.agents.includes(target));
  if (selected.length === 0) {
    throw new Error('required');
  }
  const ticketId = ticketIdFor(input.sourceKind, input.sourceId);
  const applied: CreateRouteTarget[] = [];
  try {
    for (const agent of selected) {
      await deps.planTicket(ticketId, agent);
      await deps.bindTicket(ticketId, agent);
      applied.push(agent);
    }
    return applied;
  } catch (error) {
    const compensationFailures: unknown[] = [];
    for (const agent of [...applied].reverse()) {
      try {
        await deps.unbindTicket(ticketId, agent);
      } catch (cause) {
        compensationFailures.push(cause);
      }
    }
    throw surfaceAfterCompensation(error, compensationFailures);
  }
}

export async function submitCreateRoute(
  input: CreateRouteInput,
  deps: CreateRouteDeps = defaultDeps,
  context: CreateRouteSubmitContext = {},
): Promise<CreateRouteSubmitResult> {
  if (!canSubmitCreateRoute(input)) {
    throw new Error('required');
  }
  const updateDuplicateUrl = context.policy?.updateDuplicateUrl !== false;
  const draft = createRouteProviderDraft(input);
  const existing = updateDuplicateUrl
    ? findRouteProviderByUrl(
      context.existingProviders ?? [],
      input.url,
      draft.agentId,
    )
    : undefined;
  const providerDraft = existing
    ? {
      ...draft,
      id: existing.id,
      agentId: existing.agentId,
      isCurrent: existing.isCurrent,
      official: existing.official,
      preset: existing.preset || draft.preset,
    }
    : draft;
  const provider = await deps.upsertProvider(providerDraft);
  try {
    const agents = await applyLocalRouteToAgents(
      {
        sourceKind: 'provider',
        sourceId: provider.id,
        agents: input.endpoints,
      },
      deps,
    );
    return { agents, updatedExisting: Boolean(existing) };
  } catch (error) {
    if (!existing) {
      try {
        await deps.deleteProvider(provider.agentId, provider.id);
      } catch {
        /* compensate best-effort; original error is the one to surface */
      }
    }
    throw error;
  }
}

export function importRouteTarget(agentId: AgentId): CreateRouteTarget {
  return CREATE_ROUTE_TARGETS.includes(agentId as CreateRouteTarget)
    ? agentId as CreateRouteTarget
    : 'codex';
}

export async function submitImportRoute(
  input: ImportRouteInput,
  deps: Pick<CreateRouteDeps, 'planTicket' | 'bindTicket'> = defaultDeps,
): Promise<string> {
  const ticketId = ticketIdFor(input.sourceKind, input.sourceId);
  const target = importRouteTarget(input.agentId);
  await deps.planTicket(ticketId, target as AgentId);
  await deps.bindTicket(ticketId, target as AgentId);
  return target;
}

export type EditRouteInput = {
  name: string;
  url: string;
  /** Blank means "keep the stored key". */
  key: string;
  endpoints: readonly CreateRouteTarget[];
  models?: string;
  /** Omit to keep a stored window; `auto` clears it. */
  contextWindow?: ClaudeContextWindowChoice;
  endpointUrls?: Partial<Record<CreateRouteTarget, string>>;
};

function parseRouteConfigObject(configText: string | undefined): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(configText ?? '') as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
    return parsed as Record<string, unknown>;
  } catch {
    return null;
  }
}

function distinctCreateRouteTargets(targets: readonly CreateRouteTarget[]): CreateRouteTarget[] {
  return CREATE_ROUTE_TARGETS.filter((target) => targets.includes(target));
}

function storedCreateRouteVendor(configText: string | undefined): CreateRouteVendorId {
  const vendor = readCreateRouteConfigMeta(configText).vendor;
  return CREATE_ROUTE_VENDORS.some((item) => item.id === vendor)
    ? vendor as CreateRouteVendorId
    : 'custom';
}

/** Vendor id stored on a route provider config. */
export function readStoredCreateRouteVendor(configText: string | undefined): CreateRouteVendorId {
  return storedCreateRouteVendor(configText);
}

/** True when this route's source is a provider config this dialog can edit. */
export function isEditableRouteSource(input: {
  sourceKind: 'account' | 'provider';
  provider?: Pick<Provider, 'configText' | 'configFormat'> | null;
}): boolean {
  if (input.sourceKind !== 'provider') return false;
  const provider = input.provider;
  if (!provider || provider.configFormat !== 'json') return false;
  return parseRouteConfigObject(provider.configText) !== null;
}

/** Seed the edit form from the stored provider. `key` is always '' (never echo a secret). */
export function editRouteFormFromProvider(
  provider: Pick<Provider, 'name' | 'configText'>,
): EditRouteInput {
  const caps = readCreateRouteCapabilities(provider.configText);
  const endpoints = caps.endpoints.length > 0
    ? distinctCreateRouteTargets(caps.endpoints.map((row) => row.target))
    : distinctCreateRouteTargets(
        listLocalRouteSurfacesFromConfig(provider.configText, { targetAgentId: 'codex' })
          .map((row) => row.target),
      );
  const endpointUrls = Object.fromEntries(
    caps.endpoints
      .filter((row) => row.url.trim())
      .map((row) => [row.target, row.url.trim()] as const),
  ) as Partial<Record<CreateRouteTarget, string>>;
  return {
    name: provider.name,
    url: readCreateRouteConfigMeta(provider.configText).baseUrl,
    key: '',
    endpoints,
    models: formatCreateRouteModels(caps.models),
    contextWindow: caps.contextWindow,
    endpointUrls,
  };
}

export function canSubmitEditRoute(input: EditRouteInput): boolean {
  return Boolean(
    input.name.trim()
    && isCreateRouteUrlValid(input.url)
    && input.endpoints.length > 0,
  );
}

/** Merged provider row to persist. Preserves id/agentId/preset/isCurrent/official and the stored key when `key` is blank. */
export function editRouteProviderDraft(provider: Provider, input: EditRouteInput): Provider {
  const existing = parseRouteConfigObject(provider.configText) ?? {};
  const url = normalizeCreateRouteUrl(input.url);
  const key = input.key.trim();
  const models = parseCreateRouteModels(input.models);
  const endpoints = buildCreateRouteEndpoints(
    storedCreateRouteVendor(provider.configText),
    url,
    input.endpoints,
    input.endpointUrls,
  ).filter((row) => row.enabled);
  const settings: Record<string, unknown> = {
    ...existing,
    baseURL: url,
    endpoints,
    listedModels: models,
  };
  if (key) {
    settings.apiKey = key;
  } else if (typeof settings.apiKey !== 'string' || !settings.apiKey.trim()) {
    const snakeKey = settings.api_key;
    if (typeof snakeKey === 'string' && snakeKey.trim()) settings.apiKey = snakeKey;
  }
  delete settings.baseUrl;
  delete settings.base_url;
  delete settings.api_key;
  if (models[0]) settings.model = models[0];
  else delete settings.model;
  if (input.contextWindow !== undefined) {
    const windowTokens = contextWindowTokensFromChoice(input.contextWindow);
    if (windowTokens) settings.contextWindowTokens = windowTokens;
    else delete settings.contextWindowTokens;
  }
  return {
    id: provider.id,
    agentId: provider.agentId,
    name: input.name.trim(),
    preset: provider.preset,
    configText: JSON.stringify(settings, null, 2),
    configFormat: 'json',
    isCurrent: provider.isCurrent,
    official: provider.official,
  };
}

export async function submitEditRoute(
  provider: Provider,
  input: EditRouteInput,
  deps: Pick<CreateRouteDeps, 'upsertProvider'> = defaultDeps,
): Promise<Provider> {
  if (!canSubmitEditRoute(input)) {
    throw new Error('required');
  }
  return deps.upsertProvider(editRouteProviderDraft(provider, input));
}
