// Connections：按 Agent 管理「连接」——官方登录 / API Key / 供应商统一列表。
// 存储仍为 accounts + providers 两表；本页做 UI 聚合与筛选，?mode= 仅深链提示筛选。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { AGENT_IDS, AGENT_MAP } from '@/config/agents';
import { resolveEffectiveConnection } from '@/lib/api/agent-connection';
import { listAccounts } from '@/lib/api/account';
import { listProviders } from '@/lib/api/provider';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, EffectiveConnectionKind } from '@/lib/types';
import {
  ConnectionList,
  type ConnectionPoolSnapshot,
} from './ConnectionList';
import type { ConnectionFilter } from './connection-model';

export type ConnectionMode = 'accounts' | 'providers';

function emptyCounts(ids: AgentId[]): Partial<Record<AgentId, number>> {
  const next: Partial<Record<AgentId, number>> = {};
  for (const id of ids) next[id] = 0;
  return next;
}

function parseAgentParam(raw: string | null, allowed: AgentId[]): AgentId {
  if (raw && allowed.includes(raw as AgentId)) return raw as AgentId;
  return allowed[0] ?? 'claude';
}

function pickInstalledAgent(preferred: AgentId, installed: AgentId[]): AgentId {
  const pool = installed.length ? installed : AGENT_IDS;
  if (pool.includes(preferred)) return preferred;
  return pool[0] ?? 'claude';
}

/** 深链 ?mode= → 列表初始筛选（供应商已并入 API Key） */
function parseFocusFilter(raw: string | null): ConnectionFilter | null {
  if (raw === 'providers' || raw === 'api' || raw === 'provider' || raw === 'apikey' || raw === 'key') {
    return 'apikey';
  }
  if (raw === 'accounts' || raw === 'account' || raw === 'oauth') return 'oauth';
  return null;
}

function effectiveKindLabel(kind: EffectiveConnectionKind): string {
  if (kind === 'account') return '官方登录';
  if (kind === 'api') return 'API Key';
  return '未配置';
}

export default function ConnectionsPage() {
  const { installedIds, installedAgents, statuses, loading } = useInstalledAgents();
  const [searchParams, setSearchParams] = useSearchParams();

  const rawAgent = parseAgentParam(searchParams.get('agent'), installedIds);
  const agentId = useMemo(
    () => pickInstalledAgent(rawAgent, installedIds),
    [rawAgent, installedIds],
  );
  const focusFilter = parseFocusFilter(searchParams.get('mode'));

  const installedIdsKey = installedIds.join(',');
  const installedIdsRef = useRef(installedIds);
  installedIdsRef.current = installedIds;

  const [poolCounts, setPoolCounts] = useState<Partial<Record<AgentId, number>>>({});
  /** 列表切换后即时摘要；避免只读陈旧 listAgents 的 effectiveKind */
  const [liveSnap, setLiveSnap] = useState<ConnectionPoolSnapshot | null>(null);

  const refreshCounts = useCallback(async () => {
    const ids = installedIdsRef.current.length ? installedIdsRef.current : [...AGENT_IDS];
    try {
      const [accs, provs] = await Promise.all([listAccounts(), listProviders()]);
      const totals = emptyCounts(ids);
      for (const a of accs) {
        if (a.agentId in totals || ids.includes(a.agentId)) {
          totals[a.agentId] = (totals[a.agentId] ?? 0) + 1;
        }
      }
      for (const p of provs) {
        if (p.agentId in totals || ids.includes(p.agentId)) {
          totals[p.agentId] = (totals[p.agentId] ?? 0) + 1;
        }
      }
      setPoolCounts(totals);
    } catch {
      /* 角标失败不阻塞 */
    }
  }, []);

  useEffect(() => {
    if (loading) return;
    void refreshCounts();
  }, [loading, installedIdsKey, refreshCounts]);

  // 不在换 agent 时清空 liveSnap：agentId 不匹配时自然回退 doctor 摘要，
  // 避免顶部「当前生效」先变空再填上造成闪跳。

  const handlePoolChanged = useCallback(() => {
    void refreshCounts();
  }, [refreshCounts]);

  const handleSnapshot = useCallback((snap: ConnectionPoolSnapshot) => {
    setLiveSnap(snap);
    // 角标由 onPoolChanged 链刷新；此处不再强制 refreshCounts，减少整页重绘
  }, []);

  useEffect(() => {
    if (agentId === rawAgent) return;
    const next = new URLSearchParams(searchParams);
    if (agentId === 'claude') next.delete('agent');
    else next.set('agent', agentId);
    setSearchParams(next, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId, rawAgent]);

  const setAgent = (id: AgentId) => {
    const next = new URLSearchParams(searchParams);
    if (id === 'claude') next.delete('agent');
    else next.set('agent', id);
    next.delete('mode');
    setSearchParams(next, { replace: true });
  };

  const agentStatus = statuses?.find((item) => item.agentId === agentId);
  // 优先用列表快照（切换后即时）；否则回退 doctor 富集结果
  const liveEffective =
    liveSnap && liveSnap.agentId === agentId
      ? resolveEffectiveConnection(
          liveSnap.accounts.find((a) => a.isCurrent),
          liveSnap.providers.find((p) => p.isCurrent),
        )
      : null;
  const effectiveKind: EffectiveConnectionKind =
    liveEffective?.kind ?? agentStatus?.effectiveKind ?? 'none';
  const effectiveLabel =
    liveEffective && liveEffective.kind !== 'none'
      ? liveEffective.label
      : agentStatus?.effectiveLabel;
  const agentName = AGENT_MAP[agentId]?.name ?? agentId;

  if (!loading && installedIds.length === 0) {
    return (
      <div>
        <PageHeader
          title="连接"
          description="官方登录 · API Key · 供应商"
          descriptionTip="先安装 Agent，再管理连接。"
        />
        <EmptyState
          icon={Cable}
          title="尚未安装 Agent"
          description="先到 Agents 页安装"
          actionLabel="去 Agents"
          onAction={() => {
            window.location.hash = '#/agents';
          }}
        />
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title="连接"
        description={
          effectiveKind !== 'none' && effectiveLabel
            ? `${agentName} · 当前生效：${effectiveKindLabel(effectiveKind)} · ${effectiveLabel}`
            : `${agentName} · 当前生效：未配置`
        }
        descriptionTip="官方登录与 API Key 同一列表；可填官方或中转端点。同时只能有一条生效。"
      />

      <div className={pageRhythm.chrome}>
        <AgentTabStrip
          value={agentId}
          onChange={setAgent}
          agents={installedAgents}
          aria-label="选择 Agent"
          counts={poolCounts}
          countMode="positive"
          countTitle={(_id, n) => `${n} 条连接`}
          renderEnd={(id) => {
            if (id === 'all') return null;
            const st = statuses?.find((s) => s.agentId === id);
            const hasEffective = Boolean(st?.effectiveKind && st.effectiveKind !== 'none');
            if (!hasEffective) return null;
            return (
              <span
                className="h-1.5 w-1.5 rounded-full bg-success"
                title={
                  st?.effectiveLabel
                    ? `当前生效：${st.effectiveLabel}`
                    : '已配置生效连接'
                }
                aria-hidden
              />
            );
          }}
        />
      </div>

      {/* 当前生效只保留在 PageHeader description，避免与条下横幅重复 */}
      <ConnectionList
        agentId={agentId}
        onPoolChanged={handlePoolChanged}
        onSnapshot={handleSnapshot}
        initialFilter={focusFilter ?? 'all'}
      />
    </div>
  );
}
