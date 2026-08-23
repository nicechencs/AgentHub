import { upsertProvider } from '@/lib/api/provider';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AgentId, Provider } from '@/lib/types';

export const CREATE_ROUTE_TARGETS = ['claude', 'codex', 'grok'] as const;
export const DEFAULT_CREATE_ROUTE_MODEL = 'stealth/ox-alpha';
export const DEFAULT_CREATE_ROUTE_URL = 'https://openrouter.ai/api/v1';

export type CreateRouteTarget = (typeof CREATE_ROUTE_TARGETS)[number];

export type CreateRouteVendorId = 'openrouter' | 'openai' | 'xai' | 'deepseek' | 'custom';

/** Real OpenAI-compat hosts already classified in AgentHub. Do not invent vendors. */
export const CREATE_ROUTE_VENDORS: readonly {
  id: CreateRouteVendorId;
  url: string | null;
}[] = [
  { id: 'openrouter', url: DEFAULT_CREATE_ROUTE_URL },
  { id: 'openai', url: 'https://api.openai.com/v1' },
  { id: 'xai', url: 'https://api.x.ai/v1' },
  { id: 'deepseek', url: 'https://api.deepseek.com' },
  { id: 'custom', url: null },
];

export type CreateRouteClient = {
  target: CreateRouteTarget;
  enabled: boolean;
  url: string;
};

export type CreateRouteInput = {
  name: string;
  key: string;
  clients: readonly CreateRouteClient[];
  model?: string;
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

export function defaultCreateRouteClients(): CreateRouteClient[] {
  return CREATE_ROUTE_TARGETS.map((target) => ({
    target,
    enabled: true,
    url: DEFAULT_CREATE_ROUTE_URL,
  }));
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

export function vendorIdForUrl(url: string): CreateRouteVendorId {
  const normalized = normalizeCreateRouteUrl(url).toLowerCase();
  if (!normalized) return 'openrouter';
  for (const vendor of CREATE_ROUTE_VENDORS) {
    if (vendor.url && normalizeCreateRouteUrl(vendor.url).toLowerCase() === normalized) {
      return vendor.id;
    }
  }
  return 'custom';
}

export function urlForVendor(id: CreateRouteVendorId, currentUrl: string): string {
  const vendor = CREATE_ROUTE_VENDORS.find((item) => item.id === id);
  if (!vendor?.url) return currentUrl;
  return vendor.url;
}

export function enabledCreateRouteClients(
  clients: readonly CreateRouteClient[],
): CreateRouteClient[] {
  return clients.filter((client) => client.enabled);
}

export function resolveCreateRouteTargets(
  clients: readonly CreateRouteClient[],
): CreateRouteTarget[] {
  return [...new Set(enabledCreateRouteClients(clients).map((client) => client.target))];
}

export function canSubmitCreateRoute(input: CreateRouteInput): boolean {
  if (!input.name.trim() || !input.key.trim()) return false;
  const enabled = enabledCreateRouteClients(input.clients);
  if (enabled.length === 0) return false;
  return enabled.every((client) => isCreateRouteUrlValid(client.url));
}

export function createRouteProviderDraft(
  input: CreateRouteInput,
  url: string,
  owner: CreateRouteTarget,
): Provider {
  const normalizedUrl = normalizeCreateRouteUrl(url);
  const name = input.name.trim();
  const key = input.key.trim();
  const model = input.model?.trim() || DEFAULT_CREATE_ROUTE_MODEL;
  const settings: Record<string, unknown> = {
    baseURL: normalizedUrl,
    baseUrl: normalizedUrl,
    apiKey: key,
    api_key: key,
    model,
  };
  return {
    id: `openai-compat-${crypto.randomUUID()}`,
    agentId: owner,
    name,
    preset: isOpenRouterUrl(normalizedUrl) ? 'openrouter' : 'openai-compat',
    configText: JSON.stringify(settings, null, 2),
    configFormat: 'json',
    isCurrent: false,
    official: false,
  };
}

export function groupCreateRouteClientsByUrl(
  clients: readonly CreateRouteClient[],
): { url: string; targets: CreateRouteTarget[] }[] {
  const groups = new Map<string, CreateRouteTarget[]>();
  for (const client of enabledCreateRouteClients(clients)) {
    const url = normalizeCreateRouteUrl(client.url);
    const targets = groups.get(url) ?? [];
    if (!targets.includes(client.target)) targets.push(client.target);
    groups.set(url, targets);
  }
  return [...groups.entries()].map(([url, targets]) => ({ url, targets }));
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
  const bound: string[] = [];
  for (const group of groupCreateRouteClientsByUrl(input.clients)) {
    const owner = group.targets[0] ?? 'codex';
    const provider = await deps.upsertProvider(createRouteProviderDraft(input, group.url, owner));
    const ticketId = ticketIdFor('provider', provider.id);
    for (const target of group.targets) {
      await deps.planTicket(ticketId, target as AgentId);
      await deps.bindTicket(ticketId, target as AgentId);
      bound.push(target);
    }
  }
  return bound;
}
