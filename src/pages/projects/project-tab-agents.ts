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
  if (!selectedId) return null;
  // Detect may still be running: start the scan with URL / remembered id.
  if (tabAgents.length === 0) return selectedId;
  return tabAgents.some((agent) => agent.id === selectedId) ? selectedId : null;
}

/**
 * URL `?agent=` wins; otherwise the last tab remembered in-process;
 * otherwise the first visible tab. Empty tab list keeps url/remembered so
 * the fallback can apply once detect finishes.
 */
export function resolveInitialProjectAgentId(
  agentFromUrl: string | null,
  tabAgents: readonly { id: string }[],
  remembered: string | null,
): AgentId {
  if (tabAgents.length === 0) {
    return agentFromUrl || remembered || '';
  }
  if (agentFromUrl && tabAgents.some((agent) => agent.id === agentFromUrl)) {
    return agentFromUrl;
  }
  if (remembered && tabAgents.some((agent) => agent.id === remembered)) {
    return remembered;
  }
  return tabAgents[0].id;
}
