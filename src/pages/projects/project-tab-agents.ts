import { toHiddenIdSet } from '@/lib/agent-visibility';
import type { AgentId } from '@/lib/types';

/**
 * Projects tabs: installed and not hidden only.
 * Never fall back to the full catalog — that resurfaces hidden agents when
 * nobody is installed, or while hiddenIds is still empty.
 */
export function resolveProjectTabAgents<T extends { id: string }>(
  installedAgents: readonly T[],
  hiddenIds: Iterable<string> = [],
): T[] {
  const hidden = toHiddenIdSet(hiddenIds);
  return installedAgents.filter((agent) => !hidden.has(agent.id));
}

export function resolveProjectFetchAgentId(
  tabAgents: readonly { id: string }[],
  selectedId: string,
): AgentId | null {
  if (tabAgents.length === 0) return null;
  return tabAgents.some((agent) => agent.id === selectedId) ? selectedId : null;
}
