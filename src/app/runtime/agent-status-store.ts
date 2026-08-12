/**
 * Shared Agent detection state.  Agent installation status is application
 * state, not a per-page query.  Consumers subscribe to this store so route
 * changes do not turn one detect pass into several competing requests.
 */
import type { AgentStatus } from '@/lib/types';
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
import { logger } from '@/lib/logger';

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
  clearLiveAuthProbeCache();
  setSnapshot({
    state: 'idle',
    statuses: [],
    liveAuthProbes: {},
    refreshing: false,
    error: null,
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
      // 两阶段：先放出 detect/连接池结果，主界面可立刻渲染；
      // live-auth 随后补齐，避免启动被每个已装 agent 的凭据探测拖住。
      setSnapshot({
        state: 'ready',
        statuses,
        liveAuthProbes: {},
        // 仍有 live-auth 在飞时保持 refreshing，消费者不必当整页重载。
        refreshing: true,
        error: null,
      });

      const enriched = await enrichWithLiveAuth(backend, statuses, opts.force === true);
      const next = {
        state: 'ready' as const,
        statuses: enriched.statuses,
        liveAuthProbes: enriched.liveAuthProbes,
        refreshing: false,
        error: null,
      };
      setSnapshot(next);
      return enriched.statuses;
    })
    .catch((error) => {
      log.error('agent status load failed', { errorCode: errorCode(error) });
      if (isBackgroundRefresh) {
        // The last complete data remains a more truthful UI state than a
        // transient focus-refresh failure. Restore probes as well: the
        // refresh never produced a replacement snapshot.
        setSnapshot(previousSnapshot);
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
