// Connections：按 Agent 管理「连接」——官方登录 / API Key / 供应商统一列表。
// 存储仍为 accounts + providers 两表；本页做 UI 聚合与筛选，?mode= 仅深链提示筛选。
import { useEffect, useMemo } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { StatusPin } from '@/components/shared/StatusPin';
import { ListSkeleton } from '@/components/ui/skeleton';
import { AGENT_IDS, agentDisplayName } from '@/config/agents';
import { resolveEffectiveConnection } from '@/lib/api/agent-connection';
import {
  accountsForAgent,
  connectionCountsByAgent,
  providersForAgent,
  useConnectionPool,
} from '@/app/runtime';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { connectionKindLabel, parseConnectionFocusFilter } from '@/lib/connection-kind';
import type { AgentId, EffectiveConnectionKind } from '@/lib/types';
import { ConnectionList } from './ConnectionList';
import type { ConnectionFilter } from './connection-model';

export type ConnectionMode = 'accounts' | 'providers';

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
  return parseConnectionFocusFilter(raw);
}

function effectiveKindLabel(kind: EffectiveConnectionKind): string {
  if (kind === 'account') return connectionKindLabel('oauth');
  if (kind === 'api') return connectionKindLabel('apikey');
  return '未配置';
}

export default function ConnectionsPage() {
  const { installedIds, installedAgents, statuses, loading, state, error, reload } =
    useInstalledAgents();
  const pool = useConnectionPool();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const rawAgent = parseAgentParam(searchParams.get('agent'), installedIds);
  const agentId = useMemo(
    () => pickInstalledAgent(rawAgent, installedIds),
    [rawAgent, installedIds],
  );
  const focusFilter = parseFocusFilter(searchParams.get('mode'));

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  const poolCounts = useMemo(() => {
    const ids = installedIds.length ? installedIds : [...AGENT_IDS];
    return connectionCountsByAgent(pool.accounts, pool.providers, ids);
  }, [installedIds, pool.accounts, pool.providers]);

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
  const liveEffective = resolveEffectiveConnection(
    accountsForAgent(pool.accounts, agentId).find((account) => account.isCurrent),
    providersForAgent(pool.providers, agentId).find((provider) => provider.isCurrent),
  );
  const effectiveKind: EffectiveConnectionKind =
    liveEffective.kind !== 'none' ? liveEffective.kind : agentStatus?.effectiveKind ?? 'none';
  const effectiveLabel =
    liveEffective.kind !== 'none' ? liveEffective.label : agentStatus?.effectiveLabel;
  const agentName = agentDisplayName(agentId);

  if (loading) {
    return (
      <div>
        <PageHeader
          title="连接"
          description="官方登录 · API Key"
          descriptionTip="正在检测已安装的 Agent。"
        />
        <div className={pageRhythm.chrome}>
          <ListSkeleton rows={4} />
        </div>
      </div>
    );
  }

  if (state === 'error') {
    return (
      <div>
        <PageHeader
          title="连接"
          description="官方登录 · API Key"
          descriptionTip="Agent 检测失败，请重试后再管理连接。"
        />
        <ErrorState
          error={error}
          title="无法读取 Agent 安装状态"
          onRetry={() => void reload()}
        />
      </div>
    );
  }

  if (!loading && installedIds.length === 0) {
    return (
      <div>
        <PageHeader
          title="连接"
          description="官方登录 · API Key"
          descriptionTip="先安装 Agent，再管理连接。"
        />
        <EmptyState
          icon={Cable}
          title="尚未安装 Agent"
          description="先到 Agents 页安装"
          actionLabel="去 Agents"
          onAction={() => navigate('/agents')}
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
        descriptionTip="官方登录与 API Key 在同一列表；可使用官方服务或自定义服务地址。同时只能有一条当前使用的连接。"
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
              <StatusPin
                tone="success"
                label={
                  st?.effectiveLabel
                    ? `当前生效：${st.effectiveLabel}`
                    : '已配置生效连接'
                }
              />
            );
          }}
        />
      </div>

      {/* 当前生效只保留在 PageHeader description，避免与条下横幅重复 */}
      <ConnectionList
        agentId={agentId}
        agentStatuses={statuses ?? []}
        initialFilter={focusFilter ?? 'all'}
      />
    </div>
  );
}
