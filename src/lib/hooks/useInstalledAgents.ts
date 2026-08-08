/**
 * 已安装 Agent 列表 hook。
 * Agents 页展示全量候选；其它页面应只展示 detect 结果为 installed 的 Agent。
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { AGENTS, type AgentMeta } from '@/config/agents';
import { listAgents } from '@/lib/api/agent';
import type { AgentCapabilities } from '@/lib/capability';
import type { AgentId, AgentStatus } from '@/lib/types';

export type AgentColumn = AgentMeta & {
  capabilities?: AgentCapabilities;
};

export function useInstalledAgents() {
  const [statuses, setStatuses] = useState<AgentStatus[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await listAgents();
      setStatuses(rows);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // 稳定引用：避免下游 useCallback/useEffect 因每 render 新数组而反复触发
  // （Connections 子页 load → onPoolChanged → setState 会形成加载死循环）
  const installedIds = useMemo<AgentId[]>(
    () => (statuses ?? []).filter((s) => s.installed).map((s) => s.agentId),
    [statuses],
  );

  const installedAgents = useMemo<AgentColumn[]>(
    () =>
      AGENTS.filter((a) => installedIds.includes(a.id)).map((a) => {
        // Prefer doctor/detect capabilities; never invent MOCK when statuses already loaded.
        const caps = statuses?.find((s) => s.agentId === a.id)?.capabilities;
        return { ...a, capabilities: caps };
      }),
    [installedIds, statuses],
  );

  return {
    loading,
    error,
    statuses,
    installedIds,
    installedAgents,
    reload,
  };
}
