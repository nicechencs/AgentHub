import type { AgentTabId } from '@/components/layout/AgentTabStrip';
import { toHiddenIdSet } from '@/lib/agent-visibility';

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

function isAllTab(id: string | null | undefined): id is 'all' {
  return id === 'all';
}

export function resolveProjectFetchAgentId(
  tabAgents: readonly { id: string }[],
  selectedId: string,
): AgentTabId | null {
  if (!selectedId) return null;
  if (isAllTab(selectedId)) return 'all';
  // Detect may still be running: start the scan with URL / remembered id.
  if (tabAgents.length === 0) return selectedId;
  return tabAgents.some((agent) => agent.id === selectedId) ? selectedId : null;
}

/**
 * URL `?agent=` wins (including `all`); otherwise the last tab remembered
 * in-process; otherwise 全部. Empty tab list keeps url/remembered so the
 * fallback can apply once detect finishes.
 */
export function resolveInitialProjectAgentId(
  agentFromUrl: string | null,
  tabAgents: readonly { id: string }[],
  remembered: string | null,
): AgentTabId {
  if (tabAgents.length === 0) {
    return (agentFromUrl || remembered || 'all') as AgentTabId;
  }
  if (isAllTab(agentFromUrl)) return 'all';
  if (agentFromUrl && tabAgents.some((agent) => agent.id === agentFromUrl)) {
    return agentFromUrl;
  }
  if (isAllTab(remembered)) return 'all';
  if (remembered && tabAgents.some((agent) => agent.id === remembered)) {
    return remembered;
  }
  return 'all';
}
