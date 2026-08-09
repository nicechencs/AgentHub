/**
 * Shared Agent detection state.  Agent installation status is application
 * state, not a per-page query.  Consumers subscribe to this store so route
 * changes do not turn one detect pass into several competing requests.
 */
import type { AgentStatus } from '@/lib/types';
import type { Backend } from '@/lib/backend/contracts';
import { logger } from '@/lib/logger';

const log = logger.scope('runtime:agent-status');

export type AgentStatusLoadState = 'idle' | 'loading' | 'ready' | 'error';

export interface AgentStatusSnapshot {
  state: AgentStatusLoadState;
  statuses: AgentStatus[];
  error: unknown | null;
}

type Listener = () => void;

let snapshot: AgentStatusSnapshot = {
  state: 'idle',
  statuses: [],
  error: null,
};

let inflight: Promise<AgentStatus[]> | null = null;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

function setSnapshot(next: AgentStatusSnapshot): void {
  snapshot = next;
  emit();
}

export function getAgentStatusSnapshot(): AgentStatusSnapshot {
  return snapshot;
}

export function subscribeAgentStatuses(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function resetAgentStatusStore(): void {
  inflight = null;
  setSnapshot({ state: 'idle', statuses: [], error: null });
}

export async function loadAgentStatuses(
  backend: Backend,
  opts: { force?: boolean } = {},
): Promise<AgentStatusSnapshot> {
  if (!opts.force && snapshot.state === 'ready') return snapshot;
  if (inflight) {
    const active = inflight;
    try {
      await active;
    } catch (error) {
      if (!opts.force) throw error;
    }
    if (!opts.force) return snapshot;
    // A force request that arrived during an older probe must not be lost.
    // If another waiter already started the follow-up probe, join it.
    if (inflight && inflight !== active) {
      await inflight;
      return snapshot;
    }
    return loadAgentStatuses(backend, { force: true });
  }

  setSnapshot({
    state: 'loading',
    statuses: snapshot.statuses,
    error: null,
  });

  inflight = backend.agent
    .listAgents()
    .then((statuses) => {
      const next = { state: 'ready' as const, statuses: statuses.slice(), error: null };
      setSnapshot(next);
      return statuses;
    })
    .catch((error) => {
      log.error('agent status load failed', error);
      setSnapshot({
        state: 'error',
        statuses: [],
        error,
      });
      throw error;
    })
    .finally(() => {
      inflight = null;
    });

  await inflight;
  return snapshot;
}
