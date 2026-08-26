import { AGENTS, type AgentMeta } from '@/config/agents';
import { applyIdOrder } from '@/lib/list-order';
import type { AgentId, AgentStatus, UsageRecord, UsageTrendPoint } from '@/lib/types';

export function isAgentHidden(
  status: Pick<AgentStatus, 'hidden'> | null | undefined,
): boolean {
  return Boolean(status?.hidden);
}

export function hiddenAgentIdSet(
  statuses: ReadonlyArray<Pick<AgentStatus, 'agentId' | 'hidden'>>,
): Set<AgentId> {
  return new Set(statuses.filter((row) => row.hidden).map((row) => row.agentId));
}

export function toHiddenIdSet(hiddenIds: Iterable<string>): Set<string> {
  return hiddenIds instanceof Set ? hiddenIds : new Set(hiddenIds);
}

export function visibleInstalledIds(
  statuses: ReadonlyArray<Pick<AgentStatus, 'agentId' | 'installed' | 'hidden'>>,
): AgentId[] {
  return statuses.filter((row) => row.installed && !row.hidden).map((row) => row.agentId);
}

/**
 * Ids other pages omit from lists and aggregates: uninstalled or hidden.
 * Empty while detect has not reported rows, so first paint does not blank out.
 */
export function omittedAgentIds(
  statuses: ReadonlyArray<Pick<AgentStatus, 'agentId' | 'installed' | 'hidden'>>,
): AgentId[] {
  return statuses
    .filter((row) => !row.installed || Boolean(row.hidden))
    .map((row) => row.agentId)
    .sort();
}

/**
 * Non-manage pages: drop hidden always; after detect, keep installed only.
 * While `agentsReady` is false, uninstalled rows stay so the list does not flash empty.
 */
export function isPageVisibleAgent(
  agentId: string,
  hiddenIds: ReadonlySet<string>,
  installedIds: ReadonlySet<string>,
  agentsReady: boolean,
): boolean {
  if (hiddenIds.has(agentId)) return false;
  if (!agentsReady) return true;
  return installedIds.has(agentId);
}

export function filterByPageVisibleAgent<T>(
  rows: readonly T[],
  getAgentId: (row: T) => string,
  hiddenIds: Iterable<string>,
  installedIds: readonly string[],
  agentsReady: boolean,
): T[] {
  const hidden = toHiddenIdSet(hiddenIds);
  const installed = new Set(installedIds);
  return rows.filter((row) =>
    isPageVisibleAgent(getAgentId(row), hidden, installed, agentsReady),
  );
}

export function visibleCatalogAgents(hiddenIds: Iterable<string>): AgentMeta[] {
  const hidden = toHiddenIdSet(hiddenIds);
  return AGENTS.filter((agent) => !hidden.has(agent.id));
}

export function visibleCatalogIds(hiddenIds: Iterable<string>): AgentId[] {
  return visibleCatalogAgents(hiddenIds).map((agent) => agent.id);
}

/** Management page: keep catalog order, then append hidden rows in the same order. */
export function sortAgentsForManagePage<T extends { hidden?: boolean }>(agents: T[]): T[] {
  const visible: T[] = [];
  const hidden: T[] = [];
  for (const agent of agents) {
    if (agent.hidden) hidden.push(agent);
    else visible.push(agent);
  }
  return [...visible, ...hidden];
}

/** Apply a remembered id sequence; empty storage keeps the manage-page default. */
export function applyStoredAgentOrder<T>(
  items: readonly T[],
  getId: (item: T) => string,
  stored: readonly string[],
): T[] {
  return applyIdOrder(items, getId, stored);
}

export function filterVisibleByAgentId<T extends { agentId: string }>(
  rows: readonly T[],
  hiddenIds: Iterable<string>,
): T[] {
  const hidden = toHiddenIdSet(hiddenIds);
  if (hidden.size === 0) return [...rows];
  return rows.filter((row) => !hidden.has(row.agentId));
}

export function filterVisibleUsage(
  rows: readonly UsageRecord[],
  hiddenIds: Iterable<string>,
): UsageRecord[] {
  return filterVisibleByAgentId(rows, hiddenIds);
}

export function filterVisibleTrend(
  points: readonly UsageTrendPoint[],
  hiddenIds: Iterable<string>,
): UsageTrendPoint[] {
  const hidden = toHiddenIdSet(hiddenIds);
  if (hidden.size === 0) return points.map((point) => ({ ...point }));
  return points.map((point) => {
    const next: UsageTrendPoint = { date: point.date };
    for (const [key, value] of Object.entries(point)) {
      if (key === 'date' || hidden.has(key)) continue;
      next[key] = value;
    }
    return next;
  });
}

export function firstVisibleAgentId(
  preferred: string | null | undefined,
  allowed: readonly string[],
  fallback: AgentId = 'claude',
): AgentId {
  if (preferred && allowed.includes(preferred)) return preferred as AgentId;
  return (allowed[0] as AgentId | undefined) ?? fallback;
}
