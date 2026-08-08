/**
 * Agent Catalog runtime store (non-React) + load orchestration.
 * React consumers use AgentCatalogProvider / useAgentCatalog.
 */
import { applyAgentCatalog } from '@/config/agents';
import type { Backend } from '@/lib/backend/contracts';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import { logger } from '@/lib/logger';

const log = logger.scope('runtime:agent-catalog');

export type AgentCatalogStatus =
  | 'idle'
  | 'loading'
  | 'ready'
  | 'error'
  | 'unavailable';

export interface AgentCatalogSnapshot {
  status: AgentCatalogStatus;
  entries: AgentCatalogEntryDto[];
  error: unknown | null;
  /** True after a successful applyAgentCatalog */
  hydrated: boolean;
}

type Listener = () => void;

let snapshot: AgentCatalogSnapshot = {
  status: 'idle',
  entries: [],
  error: null,
  hydrated: false,
};

const listeners = new Set<Listener>();

function emit(): void {
  for (const l of listeners) l();
}

function setSnapshot(next: AgentCatalogSnapshot): void {
  snapshot = next;
  emit();
}

export function getAgentCatalogSnapshot(): AgentCatalogSnapshot {
  return snapshot;
}

export function subscribeAgentCatalog(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Test helper: clear store and product AGENTS list. */
export function resetAgentCatalogStore(): void {
  applyAgentCatalog([]);
  snapshot = {
    status: 'idle',
    entries: [],
    error: null,
    hydrated: false,
  };
  emit();
}

/**
 * Apply entries without a backend call (mock seed / tests).
 * Marks status ready.
 */
export function seedAgentCatalog(entries: AgentCatalogEntryDto[]): void {
  applyAgentCatalog(entries);
  setSnapshot({
    status: 'ready',
    entries: entries.slice(),
    error: null,
    hydrated: true,
  });
}

/**
 * Load catalog from backend. On failure sets error/unavailable and does **not**
 * fall back to a static agent list.
 */
export async function loadAgentCatalog(backend: Backend): Promise<AgentCatalogSnapshot> {
  setSnapshot({
    status: 'loading',
    entries: snapshot.entries,
    error: null,
    hydrated: snapshot.hydrated,
  });
  try {
    const entries = await backend.catalog.listAgentCatalog();
    applyAgentCatalog(entries);
    const next: AgentCatalogSnapshot = {
      status: 'ready',
      entries: entries.slice(),
      error: null,
      hydrated: true,
    };
    setSnapshot(next);
    log.info('agent catalog loaded', { count: entries.length });
    return next;
  } catch (e) {
    log.error('agent catalog load failed', e);
    // Do not restore static AGENTS — leave product set empty / last good.
    applyAgentCatalog([]);
    const next: AgentCatalogSnapshot = {
      status: 'error',
      entries: [],
      error: e,
      hydrated: false,
    };
    setSnapshot(next);
    throw e;
  }
}
