/**
 * Shared Agent detection state.  Agent installation status is application
 * state, not a per-page query.  Consumers subscribe to this store so route
 * changes do not turn one detect pass into several competing requests.
 */
import type { AgentId, AgentStatus } from '@/lib/types';
import type { Backend } from '@/lib/backend/contracts';
import {
  authDisplayForAgentStatus,
  normalizeAuthHealth,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import {
  clearLiveAuthProbeCache,
  probeLiveAuthWithPort,
} from '@/lib/backend/contracts/live-auth-probe-cache';
import type { LiveAuthProbe } from '@/lib/backend/contracts/ports';
import { enrichStatusesWithConnections } from '@/lib/backend/contracts/agent-connection';
import { logger } from '@/lib/logger';
import {
  getConnectionPoolSnapshot,
  loadConnectionPool,
} from './connection-pool-store';

const log = logger.scope('runtime:agent-status');

export type AgentStatusLoadState = 'idle' | 'loading' | 'ready' | 'error';

export interface AgentStatusSnapshot {
  state: AgentStatusLoadState;
  statuses: AgentStatus[];
  /** Complete, agent-bound live probes for UI flows that need probe kind. */
  liveAuthProbes: Readonly<Record<string, LiveAuthProbe | undefined>>;
  /**
   * A forced reconciliation is checking the machine again while the last
   * complete snapshot remains safe to render. This is deliberately separate
   * from `state`: consumers must not treat an external credential refresh as
   * an initial page load.
   */
  refreshing: boolean;
  error: unknown | null;
}

type Listener = () => void;

let snapshot: AgentStatusSnapshot = {
  state: 'idle',
  statuses: [],
  liveAuthProbes: {},
  refreshing: false,
  error: null,
};

let inflight: Promise<AgentStatus[]> | null = null;
/** In-flight / unconfirmed visibility writes. Survives a stale listAgents. */
const pendingHidden = new Map<AgentId, boolean>();
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

/**
 * Return a probe only when both the cache key and wire payload identify the
 * requested agent. This prevents a late or malformed result from authorizing
 * an import for a different agent.
 */
export function liveAuthProbeForAgent(
  source: Pick<AgentStatusSnapshot, 'liveAuthProbes'>,
  agentId: string,
): LiveAuthProbe | undefined {
  const probe = source.liveAuthProbes[agentId];
  return probe?.agentId === agentId ? probe : undefined;
}

export function resetAgentStatusStore(): void {
  inflight = null;
  pendingHidden.clear();
  clearLiveAuthProbeCache();
  setSnapshot({
    state: 'idle',
    statuses: [],
    liveAuthProbes: {},
    refreshing: false,
    error: null,
  });
}

/**
 * Stamp a soft-hide preference onto the shared snapshot immediately.
 * A later listAgents that still carries the old bit cannot clobber this
 * until the backend result matches (or {@link revertAgentHidden} runs).
 */
export function applyAgentHidden(agentId: AgentId, hidden: boolean): void {
  pendingHidden.set(agentId, hidden);
  const current = snapshot.statuses.find((row) => row.agentId === agentId);
  if (!current || Boolean(current.hidden) === hidden) return;
  setSnapshot({
    ...snapshot,
    statuses: snapshot.statuses.map((row) =>
      row.agentId === agentId ? { ...row, hidden } : row,
    ),
  });
}

/** Undo {@link applyAgentHidden} when persist fails. */
export function revertAgentHidden(agentId: AgentId, previous: boolean): void {
  pendingHidden.delete(agentId);
  setSnapshot({
    ...snapshot,
    statuses: snapshot.statuses.map((row) =>
      row.agentId === agentId ? { ...row, hidden: previous } : row,
    ),
  });
}

function settleConfirmedHidden(statuses: AgentStatus[]): void {
  for (const status of statuses) {
    const pending = pendingHidden.get(status.agentId);
    if (pending !== undefined && Boolean(status.hidden) === pending) {
      pendingHidden.delete(status.agentId);
    }
  }
}

function applyPendingHidden(statuses: AgentStatus[]): AgentStatus[] {
  if (pendingHidden.size === 0) return statuses;
  return statuses.map((status) => {
    const pending = pendingHidden.get(status.agentId);
    if (pending === undefined || Boolean(status.hidden) === pending) return status;
    return { ...status, hidden: pending };
  });
}

function healthFromLiveProbe(probe: LiveAuthProbe): AuthHealth {
  const explicit = normalizeAuthHealth(probe.health);
  if (explicit) return explicit;
  if (!probe.hasCredentials) return 'missing';
  const kind = probe.kind?.trim().toLowerCase();
  if (kind === 'api_key' || kind === 'api-key' || kind === 'apikey') return 'configured';
  // A live OAuth/file credential exists, but old ports did not verify it.
  return 'unknown';
}

/**
 * Live probe data is the canonical auth signal for the current machine. Keep
 * doctor/effective-connection fields only as a compatibility projection.
 */
export function mergeLiveAuthIntoAgentStatus(
  status: AgentStatus,
  probe: LiveAuthProbe,
): AgentStatus {
  const authHealth = healthFromLiveProbe(probe);
  const display = authDisplayForAgentStatus({ ...status, authHealth });
  return {
    ...status,
    authHealth,
    authHealthLabel: display.label,
    authStatus: display.legacyStatus,
    authLabel: display.label,
    authSource: probe.source ?? status.authSource,
    authRevision: probe.revision ?? status.authRevision,
  };
}

async function enrichWithLiveAuth(
  backend: Backend,
  statuses: AgentStatus[],
  force: boolean,
): Promise<{ statuses: AgentStatus[]; liveAuthProbes: Record<string, LiveAuthProbe> }> {
  if (!backend.account?.probeLiveAuth) return { statuses, liveAuthProbes: {} };
  const probes = await Promise.all(
    statuses.map(async (status) => {
      if (!status.installed) return undefined;
      try {
        return await probeLiveAuthWithPort(backend.account, status.agentId, { force });
      } catch (error) {
        // Agent detection remains useful when one auth file is inaccessible.
        log.warn('live auth probe failed; retaining compatibility status', {
          agentId: status.agentId,
          source: 'live-auth',
          errorCode: errorCode(error),
        });
        return undefined;
      }
    }),
  );
  const liveAuthProbes: Record<string, LiveAuthProbe> = {};
  const enriched = statuses.map((status, index) => {
    const probe = probes[index];
    if (!probe) return status;
    if (probe.agentId !== status.agentId) {
      log.warn('live auth probe agent mismatch; ignored', {
        agentId: status.agentId,
        kind: probe.kind ?? 'unknown',
      });
      return status;
    }
    liveAuthProbes[status.agentId] = probe;
    return mergeLiveAuthIntoAgentStatus(status, probe);
  });
  return { statuses: enriched, liveAuthProbes };
}

function canLoadConnectionPool(backend: Backend): boolean {
  return (
    typeof backend.account?.listAccounts === 'function' &&
    typeof backend.provider?.listProviders === 'function'
  );
}

async function mergeConnectionPool(
  backend: Backend,
  statuses: AgentStatus[],
): Promise<AgentStatus[]> {
  if (!canLoadConnectionPool(backend)) return statuses;
  try {
    const pool = await loadConnectionPool(backend);
    return enrichStatusesWithConnections(statuses, pool.accounts, pool.providers);
  } catch (error) {
    log.warn('connection pool merge failed; showing detect-only status', {
      errorCode: errorCode(error),
    });
    const pool = getConnectionPoolSnapshot();
    return enrichStatusesWithConnections(statuses, pool.accounts, pool.providers);
  }
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

  const previousSnapshot = snapshot;
  const isBackgroundRefresh = opts.force === true && previousSnapshot.state === 'ready';

  if (isBackgroundRefresh) {
    // Window focus and credential rotation should not tear down consumers that
    // already have a complete detection result. The stale probes are cleared
    // so mutation gates remain fail-closed until this refresh completes.
    setSnapshot({
      ...previousSnapshot,
      liveAuthProbes: {},
      refreshing: true,
      error: null,
    });
  } else {
    setSnapshot({
      state: 'loading',
      statuses: previousSnapshot.statuses,
      liveAuthProbes: {},
      refreshing: false,
      error: null,
    });
  }

  inflight = backend.agent
    .listAgents()
    .then(async (statuses) => {
      // 三阶段：detect 先上屏；连接池复用共享 store；live-auth 最后补齐。
      settleConfirmedHidden(statuses);
      setSnapshot({
        state: 'ready',
        statuses: applyPendingHidden(statuses),
        liveAuthProbes: {},
        refreshing: true,
        error: null,
      });

      const withPool = await mergeConnectionPool(backend, statuses);
      setSnapshot({
        state: 'ready',
        statuses: applyPendingHidden(withPool),
        liveAuthProbes: {},
        refreshing: true,
        error: null,
      });

      const enriched = await enrichWithLiveAuth(backend, withPool, opts.force === true);
      const nextStatuses = applyPendingHidden(enriched.statuses);
      const next = {
        state: 'ready' as const,
        statuses: nextStatuses,
        liveAuthProbes: enriched.liveAuthProbes,
        refreshing: false,
        error: null,
      };
      setSnapshot(next);
      return nextStatuses;
    })
    .catch((error) => {
      log.error('agent status load failed', { errorCode: errorCode(error) });
      if (isBackgroundRefresh) {
        // The last complete data remains a more truthful UI state than a
        // transient focus-refresh failure. Restore probes as well: the
        // refresh never produced a replacement snapshot. Re-apply a hide
        // that landed while this refresh was in flight.
        setSnapshot({
          ...previousSnapshot,
          statuses: applyPendingHidden(previousSnapshot.statuses),
        });
      } else {
        setSnapshot({
          state: 'error',
          statuses: [],
          liveAuthProbes: {},
          refreshing: false,
          error,
        });
      }
      throw error;
    })
    .finally(() => {
      inflight = null;
    });

  await inflight;
  return snapshot;
}

function errorCode(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return error.code;
  }
  if (error instanceof Error && error.name) return error.name;
  return 'unknown';
}
