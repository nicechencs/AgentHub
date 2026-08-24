import { upsertProvider } from '@/lib/api/provider';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AgentId, Provider } from '@/lib/types';

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

export function defaultCreateRouteName(vendorLabel: string, alternateLabel: string): string {
  return `${vendorLabel.trim()} ${alternateLabel.trim()}`.replace(/\s+/g, ' ').trim();
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

export function isAlternateRouteRule(ruleId: string | null | undefined): boolean {
  return ruleId === 'openai-api-to-claude-v1'
    || ruleId === 'openai-api-to-codex-v1'
    || ruleId === 'openai-api-to-grok-bridge-v1';
}

export async function submitCreateRoute(
  input: CreateRouteInput,
  deps: CreateRouteDeps = defaultDeps,
): Promise<string[]> {
  if (!canSubmitCreateRoute(input)) {
    throw new Error('required');
  }
  const provider = await deps.upsertProvider(createRouteProviderDraft(input));
  const owner = createRouteOwner(input.endpoints);
  const ticketId = ticketIdFor('provider', provider.id);
  await deps.planTicket(ticketId, owner as AgentId);
  await deps.bindTicket(ticketId, owner as AgentId);
  return [owner];
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
