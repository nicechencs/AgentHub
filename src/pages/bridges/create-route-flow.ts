import { upsertProvider } from '@/lib/api/provider';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AgentId, Provider } from '@/lib/types';

/** Bind these when the agent is installed. Pi is config_sync; Codex/Grok are 本机路由. */
export const CREATE_ROUTE_BIND_TARGETS = ['codex', 'grok', 'pi'] as const;

export const OPENROUTER_BACKUP_MODEL = 'stealth/ox-alpha';

export type CreateRouteBindTarget = (typeof CREATE_ROUTE_BIND_TARGETS)[number];

export type CreateRouteInput = {
  name: string;
  url: string;
  key: string;
  /** Installed agent ids. Empty means none live. */
  liveAgents: readonly string[];
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

export function normalizeCreateRouteUrl(url: string): string {
  return url.trim().replace(/\/+$/, '');
}

export function liveCreateRouteTargets(
  liveAgents: readonly string[],
): CreateRouteBindTarget[] {
  return CREATE_ROUTE_BIND_TARGETS.filter((target) => liveAgents.includes(target));
}

export function createRouteProviderDraft(input: CreateRouteInput): Provider {
  const url = normalizeCreateRouteUrl(input.url);
  const name = input.name.trim();
  const key = input.key.trim();
  return {
    id: `p-${Date.now()}`,
    agentId: 'codex',
    name,
    preset: 'openai-compatible',
    configText: JSON.stringify(
      {
        baseURL: url,
        apiKey: key,
        model: OPENROUTER_BACKUP_MODEL,
      },
      null,
      2,
    ),
    configFormat: 'json',
    isCurrent: false,
    official: false,
    authApiKey: key,
  };
}

/**
 * Save a custom OpenAI-compat login, then plan/bind Codex, Grok, and Pi when live.
 * Port is assigned by the host saga; this does not take a user port.
 */
export async function submitCreateRoute(
  input: CreateRouteInput,
  deps: CreateRouteDeps = defaultDeps,
): Promise<CreateRouteBindTarget[]> {
  const targets = liveCreateRouteTargets(input.liveAgents);
  if (!input.name.trim() || !normalizeCreateRouteUrl(input.url) || !input.key.trim()) {
    throw new Error('required');
  }
  if (targets.length === 0) {
    throw new Error('none-live');
  }
  const provider = await deps.upsertProvider(createRouteProviderDraft(input));
  const ticketId = ticketIdFor('provider', provider.id);
  const bound: CreateRouteBindTarget[] = [];
  let lastError: unknown;
  for (const target of targets) {
    try {
      const plan = await deps.planTicket(ticketId, target as AgentId);
      if (!plan.canApply) continue;
      await deps.bindTicket(ticketId, target as AgentId);
      bound.push(target);
    } catch (error) {
      lastError = error;
    }
  }
  if (bound.length === 0) {
    if (lastError instanceof Error) throw lastError;
    throw new Error('none-bound');
  }
  return bound;
}
