import { upsertProvider } from '@/lib/api/provider';
import { bindTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AgentId, Provider } from '@/lib/types';

export const CREATE_ROUTE_TARGETS = ['claude', 'codex', 'grok'] as const;
export const DEFAULT_CREATE_ROUTE_MODEL = 'stealth/ox-alpha';

export type CreateRouteTarget = (typeof CREATE_ROUTE_TARGETS)[number];

export type CreateRouteInput = {
  name: string;
  url: string;
  key: string;
  /** Empty = bind Claude, Codex, and Grok. */
  targets: readonly CreateRouteTarget[];
  model?: string;
};

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

export function canSubmitCreateRoute(input: CreateRouteInput): boolean {
  return Boolean(input.name.trim() && input.url.trim() && input.key.trim())
    && isCreateRouteUrlValid(input.url);
}

export function createRouteProviderDraft(input: CreateRouteInput): Provider {
  const url = normalizeCreateRouteUrl(input.url);
  const name = input.name.trim();
  const key = input.key.trim();
  const model = input.model?.trim() || DEFAULT_CREATE_ROUTE_MODEL;
  const owner = input.targets[0] ?? 'codex';
  const settings: Record<string, unknown> = {
    baseURL: url,
    baseUrl: url,
    apiKey: key,
    api_key: key,
    model,
  };
  return {
    id: `openai-compat-${crypto.randomUUID()}`,
    agentId: owner,
    name,
    preset: isOpenRouterUrl(url) ? 'openrouter' : 'openai-compat',
    configText: JSON.stringify(settings, null, 2),
    configFormat: 'json',
    isCurrent: false,
    official: false,
  };
}

export function resolveCreateRouteTargets(
  targets: readonly CreateRouteTarget[],
): CreateRouteTarget[] {
  if (targets.length === 0) return [...CREATE_ROUTE_TARGETS];
  return [...new Set(targets)];
}

export function isAlternateRouteRule(ruleId: string | null | undefined): boolean {
  return ruleId === 'openai-api-to-claude-v1'
    || ruleId === 'openai-api-to-codex-v1'
    || ruleId === 'openai-api-to-grok-bridge-v1';
}

export async function submitCreateRoute(input: CreateRouteInput): Promise<string[]> {
  if (!canSubmitCreateRoute(input)) {
    throw new Error('required');
  }
  const provider = await upsertProvider(createRouteProviderDraft(input));
  const ticketId = ticketIdFor('provider', provider.id);
  const bound: string[] = [];
  for (const target of resolveCreateRouteTargets(input.targets)) {
    await bindTicket(ticketId, target as AgentId);
    bound.push(target);
  }
  return bound;
}
