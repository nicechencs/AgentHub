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
