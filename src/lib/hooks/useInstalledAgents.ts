/**
 * 已安装 Agent 列表 hook。
 * Agents 页展示全量候选；其它页面应只展示 detect 结果为 installed 且未隐藏的 Agent。
 */
import { useMemo } from 'react';
import { useAgentStatuses } from '@/app/runtime';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { AGENTS, type AgentMeta } from '@/config/agents';
import {
  applyStoredAgentOrder,
  hiddenAgentIdSet,
  omittedAgentIds,
  visibleCatalogIds,
  visibleInstalledIds,
} from '@/lib/agent-visibility';
import type { AgentCapabilities } from '@/lib/capability';
import type { AgentId } from '@/lib/types';
import { StorageKey } from '@/lib/ui-preferences';

export type AgentColumn = AgentMeta & {
  capabilities?: AgentCapabilities;
};

export function useInstalledAgents() {
  const { state, statuses, error, reload } = useAgentStatuses();
  const { stored: catalogOrder } = useStoredIdOrder(StorageKey.agentsCatalogOrder);

  // 稳定引用：避免下游 useCallback/useEffect 因每 render 新数组而反复触发
  // （Connections 子页 load → onPoolChanged → setState 会形成加载死循环）
  const hiddenIds = useMemo<AgentId[]>(
    () => [...hiddenAgentIdSet(statuses)],
    [statuses],
  );

  const omittedIds = useMemo<AgentId[]>(
    () => omittedAgentIds(statuses),
    [statuses],
  );

  const installedIds = useMemo<AgentId[]>(
    () => applyStoredAgentOrder(visibleInstalledIds(statuses), (id) => id, catalogOrder),
    [catalogOrder, statuses],
  );

  const visibleIds = useMemo<AgentId[]>(
    () => applyStoredAgentOrder(visibleCatalogIds(hiddenIds), (id) => id, catalogOrder),
    [catalogOrder, hiddenIds],
  );

  const installedAgents = useMemo<AgentColumn[]>(
    () =>
      applyStoredAgentOrder(
        AGENTS.filter((a) => installedIds.includes(a.id)).map((a) => {
          // Prefer doctor/detect capabilities; never invent MOCK when statuses already loaded.
          const caps = statuses?.find((s) => s.agentId === a.id)?.capabilities;
          return { ...a, capabilities: caps };
        }),
        (agent) => agent.id,
        catalogOrder,
      ),
    [catalogOrder, installedIds, statuses],
  );

  return {
    loading: state === 'idle' || state === 'loading',
    state,
    error,
    statuses,
    hiddenIds,
    omittedIds,
    visibleIds,
    installedIds,
    installedAgents,
    reload,
  };
}
