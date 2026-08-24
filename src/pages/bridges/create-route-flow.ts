import { upsertProvider } from '@/lib/api/provider';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import {
  isInternalGeneratedProvider,
  isLoopbackUrl,
} from '@/lib/backend/contracts/agent-connection';
import type { RouteEndpointId } from '@/lib/route-endpoints';
import { routeEndpointIdForBinding, routeEndpointPath } from '@/lib/route-endpoints';
import type { AgentId, Provider } from '@/lib/types';
import { isLeftoverLocalRouteProvider } from '@/pages/chat/chat-model';

export const CREATE_ROUTE_TARGETS = ['claude', 'codex', 'grok'] as const;
export const DEFAULT_CREATE_ROUTE_MODEL = 'stealth/ox-alpha';
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
  endpointUrls?: Partial<Record<CreateRouteTarget, string>>;
};

export const CREATE_ROUTE_VENDORS: readonly CreateRouteVendor[] = [
  {
    id: 'openrouter',
    url: DEFAULT_CREATE_ROUTE_URL,
    enabled: ['claude', 'codex', 'grok'],
    models: [DEFAULT_CREATE_ROUTE_MODEL],
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
    enabled: ['claude', 'codex', 'grok'],
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
};

const defaultDeps: CreateRouteDeps = {
  upsertProvider,
  planTicket,
  bindTicket,
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
  return text
    .split(/[,，\n]+/)
    .map((item) => item.trim())
    .filter(Boolean);
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
): T[] {
  const routedTitles = new Set<string>();
  for (const entry of entries) {
    if (entry.source !== 'provider') continue;
    const title = entry.title?.trim();
    if (!title) continue;
    const key = connectionSourceKey(entry.source, entry.id);
    const occupied = routedKeys.has(key)
      || profiles.some((profile) => entryMatchesRoutedProfile(entry, profile));
    if (occupied) routedTitles.add(title);
  }
  return entries.filter((entry) => {
    if (routedKeys.has(connectionSourceKey(entry.source, entry.id))) return false;
    if (profiles.some((profile) => entryMatchesRoutedProfile(entry, profile))) return false;
    if (entry.source === 'provider') {
      const title = entry.title?.trim();
      if (title && routedTitles.has(title)) return false;
    }
    return !isGeneratedLocalRouteEntry(entry);
  });
}

export function endpointUrlFor(
  vendor: CreateRouteVendorId,
  target: CreateRouteTarget,
  formUrl: string,
): string {
  const spec = vendorById(vendor);
  const primary = normalizeCreateRouteUrl(spec.url);
  const current = normalizeCreateRouteUrl(formUrl);
  const specific = spec.endpointUrls?.[target];
  if (specific && (!current || current === primary || current === normalizeCreateRouteUrl(specific))) {
    return specific;
  }
  return formUrl.trim();
}

export function buildCreateRouteEndpoints(
  vendor: CreateRouteVendorId,
  formUrl: string,
  enabled: readonly CreateRouteTarget[],
): CreateRouteEndpoint[] {
  return CREATE_ROUTE_TARGETS.map((target) => ({
    target,
    enabled: enabled.includes(target),
    url: endpointUrlFor(vendor, target, formUrl),
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
  const endpoints = buildCreateRouteEndpoints(input.vendor, url, input.endpoints)
    .filter((row) => row.enabled);
  const settings: Record<string, unknown> = {
    baseURL: url,
    baseUrl: url,
    apiKey: key,
    api_key: key,
    vendor: input.vendor,
    endpoints,
    listedModels: models,
  };
  if (models[0]) settings.model = models[0];
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
} {
  try {
    const parsed = JSON.parse(configText ?? '{}') as {
      listedModels?: unknown;
      model?: unknown;
      endpoints?: unknown;
    };
    const models = Array.isArray(parsed.listedModels)
      ? parsed.listedModels.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
      : parseCreateRouteModels(typeof parsed.model === 'string' ? parsed.model : '');
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
    return { endpoints: endpoints.filter((row) => row.enabled), models };
  } catch {
    return { endpoints: [], models: [] };
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

export async function applyLocalRouteToAgents(
  input: {
    sourceKind: 'account' | 'provider';
    sourceId: string;
    agents: readonly CreateRouteTarget[];
  },
  deps: Pick<CreateRouteDeps, 'planTicket' | 'bindTicket'> = defaultDeps,
): Promise<CreateRouteTarget[]> {
  const selected = CREATE_ROUTE_TARGETS.filter((target) => input.agents.includes(target));
  if (selected.length === 0) {
    throw new Error('required');
  }
  const ticketId = ticketIdFor(input.sourceKind, input.sourceId);
  for (const agent of selected) {
    await deps.planTicket(ticketId, agent);
    await deps.bindTicket(ticketId, agent);
  }
  return selected;
}

export async function submitCreateRoute(
  input: CreateRouteInput,
  deps: CreateRouteDeps = defaultDeps,
): Promise<string[]> {
  if (!canSubmitCreateRoute(input)) {
    throw new Error('required');
  }
  const provider = await deps.upsertProvider(createRouteProviderDraft(input));
  return applyLocalRouteToAgents(
    {
      sourceKind: 'provider',
      sourceId: provider.id,
      agents: input.endpoints,
    },
    deps,
  );
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
  return {
    name: provider.name,
    url: readCreateRouteConfigMeta(provider.configText).baseUrl,
    key: '',
    endpoints,
    models: formatCreateRouteModels(caps.models),
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
  ).filter((row) => row.enabled);
  const settings: Record<string, unknown> = {
    ...existing,
    baseURL: url,
    baseUrl: url,
    endpoints,
    listedModels: models,
  };
  // `extractProviderEndpoint` reads JSON `base_url`, so a stored one must not go stale.
  if ('base_url' in existing) settings.base_url = url;
  if (key) {
    settings.apiKey = key;
    settings.api_key = key;
  }
  if (models[0]) settings.model = models[0];
  else delete settings.model;
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
