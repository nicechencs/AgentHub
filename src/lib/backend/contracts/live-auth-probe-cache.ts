import type { AgentId } from '@/lib/types';
import {
  normalizeAuthState,
  type AccountPort,
  type AuthState,
  type LiveAuthProbe,
} from './ports';

export interface ProbeLiveAuthOptions {
  /** Bypass the short-lived result and supersede any active request. */
  force?: boolean;
}

type PendingProbe = {
  generation: number;
  token: number;
  promise: Promise<LiveAuthProbe>;
};

type ProbeCacheEntry = {
  at?: number;
  value?: LiveAuthProbe;
  pending?: PendingProbe;
};

const LIVE_PROBE_CACHE_MS = 2500;
const cache = new Map<AgentId, ProbeCacheEntry>();
const generations = new Map<AgentId, number>();
let nextRequestToken = 0;

function generationFor(agentId: AgentId): number {
  return generations.get(agentId) ?? 0;
}

function invalidateGeneration(agentId: AgentId): void {
  generations.set(agentId, generationFor(agentId) + 1);
  cache.delete(agentId);
}

/**
 * Shared live-auth probe cache. A request may update the cache only when it
 * is still the newest request for the agent and no clear happened meanwhile.
 * This matters when a forced focus refresh races an older regular probe.
 */
export function probeLiveAuthWithPort(
  port: Pick<AccountPort, 'probeLiveAuth'>,
  agentId: AgentId,
  options: ProbeLiveAuthOptions = {},
): Promise<LiveAuthProbe> {
  const now = Date.now();
  const entry = cache.get(agentId);
  const generation = generationFor(agentId);

  if (!options.force && entry?.value && entry.at !== undefined && now - entry.at < LIVE_PROBE_CACHE_MS) {
    return Promise.resolve(entry.value);
  }
  if (!options.force && entry?.pending && entry.pending.generation === generation) {
    return entry.pending.promise;
  }

  const token = ++nextRequestToken;
  // Start through Promise.resolve so a legacy port that throws synchronously
  // is handled like a rejected probe and cannot leave a stale pending entry.
  const promise = Promise.resolve()
    .then(() => port.probeLiveAuth(agentId))
    .then((raw) => {
      const normalized = normalizeAuthState(
        raw as LiveAuthProbe & Partial<AuthState>,
        agentId,
      );
      const current = cache.get(agentId);
      if (
        generationFor(agentId) === generation &&
        current?.pending?.generation === generation &&
        current.pending.token === token
      ) {
        cache.set(agentId, { at: Date.now(), value: normalized });
      }
      return normalized;
    })
    .finally(() => {
      const current = cache.get(agentId);
      if (
        generationFor(agentId) === generation &&
        current?.pending?.generation === generation &&
        current.pending.token === token
      ) {
        // Successful requests have already replaced pending with value. A
        // rejected request should simply make the next caller retry.
        cache.delete(agentId);
      }
    });

  cache.set(agentId, {
    pending: { generation, token, promise },
  });
  return promise;
}

/** Invalidate a single agent or all agents, superseding in-flight probes. */
export function clearLiveAuthProbeCache(agentId?: AgentId): void {
  if (agentId) {
    invalidateGeneration(agentId);
    return;
  }

  const ids = new Set<AgentId>([...cache.keys(), ...generations.keys()]);
  for (const id of ids) invalidateGeneration(id);
  cache.clear();
}
