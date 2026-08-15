import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import {
  ArrowLeftRight,
  BarChart3,
  DatabaseBackup,
  RefreshCw,
  UserRound,
} from 'lucide-react';

import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { UsageParserHealth } from '@/components/shared/UsageParserHealth';
import { useUsageSync } from '@/components/shared/UsageSyncProvider';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Tip } from '@/components/ui/tooltip';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton, TableSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';

import { listAgents } from '@/lib/api/agent';
import {
  getAdapterBridgeStatus,
  listAdapterProfiles,
  type AdapterBridgeRuntimeState,
  type AdapterProfile,
} from '@/lib/api/adapter';
import { listRuntimes } from '@/lib/api/env';
import {
  getUsageAvailability,
  listModels,
  queryUsage,
  usageTrend,
  type UsageAvailability,
} from '@/lib/api/usage';
import { createBackup } from '@/lib/api/backup';
import { listTicketWallet, type TicketWallet } from '@/lib/api/tickets';
import { bindingRouteDashboardLabel } from '@/lib/backend/contracts/ticket';
import { activeBindingForAgent } from '@/lib/ticket-wallet';
import { ConnectFlowDialog } from '@/components/connect/ConnectFlowDialog';
import { consumeConnectResume, parseConnectResumeParam } from '@/lib/connect-flow/connect-intent';
import { createDefaultConnectFlowDeps } from '@/lib/connect-flow/default-deps';
import type { ConnectFlowEntry } from '@/lib/connect-flow/types';
import { getConnectionPoolSnapshot, providersForAgent, useConnectionPool } from '@/app/runtime';
import { AGENTS, AGENT_MAP, agentDisplayName } from '@/config/agents';
import { hasEnvIssues } from '@/lib/env';
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';
import type { AgentId, AgentStatus, RuntimeDetect, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { USAGE_COLLECTED_EVENT } from '@/lib/usage-sync';
import { usageTokenParts } from '@/lib/usage-tokens';
import { cn, fmtTokens } from '@/lib/utils';
import { AgentOverview, AgentOverviewSkeleton } from './AgentOverview';
import type { AgentCardBadgeInput, AgentCardBridgeState } from './agentOverviewModel';
import { UsageDetailsTable } from './UsageDetailsTable';

/** 日期筛选预设：today / 24h 均按 days=1 拉取，today 再按本地日历日收窄 */
type DateRange = 'today' | '24h' | '7d' | '30d';

const DATE_RANGE_OPTIONS: { value: DateRange; label: string; days: number }[] = [
  { value: 'today', label: '今天', days: 1 },
  { value: '24h', label: '近 24 小时', days: 1 },
  { value: '7d', label: '7 天', days: 7 },
  { value: '30d', label: '一个月', days: 30 },
];

function isLocalToday(iso: string): boolean {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return false;
  const now = new Date();
  return (
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate()
  );
}

function localDateKey(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** 桥运行态 → 卡片徽标四态：starting 乐观归 running，error 归 degraded（可见异常） */
function mapBridgeState(state: AdapterBridgeRuntimeState): AgentCardBridgeState {
  if (state === 'running' || state === 'starting') return 'running';
  if (state === 'degraded' || state === 'error') return 'degraded';
  return 'stopped';
}

/** 桥状态轮询间隔，与 Adapter 页 use-adapter-resources 一致 */
const BRIDGE_POLL_MS = 4_000;

export default function DashboardPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { toast } = useToast();
  const usageSync = useUsageSync();
  const usageSectionRef = useRef<HTMLElement>(null);

  // —— Agent / runtime（上半）——
  const [agents, setAgents] = useState<AgentStatus[] | null>(null);
  const [runtimes, setRuntimes] = useState<RuntimeDetect[]>([]);
  const [agentsLoading, setAgentsLoading] = useState(true);
  const [agentsError, setAgentsError] = useState<unknown>(null);
  const [backingUp, setBackingUp] = useState(false);

  // —— 页面级共享筛选（时间 + Agent；模型仅作用于明细表，但 UI 与前两者并排）——
  const [dateRange, setDateRange] = useState<DateRange>('7d');
  const [agentFilter, setAgentFilter] = useState<AgentId | 'all'>('all');
  const [modelFilter, setModelFilter] = useState<string>('all');
  const [models, setModels] = useState<string[]>([]);

  // —— 用量数据（全页一份）——
  const [usageAvailability, setUsageAvailability] = useState<UsageAvailability | null>(null);
  const [usage, setUsage] = useState<UsageRecord[] | null>(null);
  const [trend, setTrend] = useState<UsageTrendPoint[]>([]);
  const [usageLoading, setUsageLoading] = useState(true);
  const [usageRefreshing, setUsageRefreshing] = useState(false);
  const [usageError, setUsageError] = useState<unknown>(null);

  const days =
    DATE_RANGE_OPTIONS.find((o) => o.value === dateRange)?.days ?? 7;
  const dayLabel =
    DATE_RANGE_OPTIONS.find((o) => o.value === dateRange)?.label ?? '';

  // —— 采集（状态由 UsageSyncProvider 统一管理）——
  const collecting = usageSync.collecting;
  const collectPct = usageSync.collectPct;
  const [healthRefreshKey, setHealthRefreshKey] = useState(0);
  const [showGuide, setShowGuide] = useState(() => !loadBool(StorageKey.usageGuideDismissed));

  /** 返回是否成功：连接流程的刷新契约需要真实成败，不能吞掉失败。 */
  const loadAgents = useCallback(async (): Promise<boolean> => {
    setAgentsLoading(true);
    setAgentsError(null);
    try {
      const [agentList, runtimeList] = await Promise.all([listAgents(), listRuntimes()]);
      setAgents(agentList);
      setRuntimes(runtimeList);
      return true;
    } catch (e) {
      setAgentsError(e);
      return false;
    } finally {
      setAgentsLoading(false);
    }
  }, []);

  // —— 连接流程（Hub 主入口）：卡片徽标数据 + ConnectFlowDialog 接线 ——
  const pool = useConnectionPool();
  const [profiles, setProfiles] = useState<AdapterProfile[]>([]);
  const [wallet, setWallet] = useState<TicketWallet | null>(null);
  const [walletError, setWalletError] = useState<unknown>(null);
  const [connectEntry, setConnectEntry] = useState<ConnectFlowEntry | null>(null);
  const [bridgeStates, setBridgeStates] = useState<Record<string, AgentCardBridgeState>>({});
  const connectDeps = useMemo(() => createDefaultConnectFlowDeps(), []);
  const poolReload = pool.reload;
  const poolEnsureLoaded = pool.ensureLoaded;
  const poolState = pool.state;

  /** generation 防竞态：并发加载只让最新一次落盘；返回是否成功。 */
  const profilesGeneration = useRef(0);
  const loadProfiles = useCallback(async (): Promise<boolean> => {
    const generation = ++profilesGeneration.current;
    try {
      const list = await listAdapterProfiles();
      if (profilesGeneration.current === generation) setProfiles(list);
      return true;
    } catch {
      // 徽标是增强信息：读取失败降级为不显示，不阻塞总览
      if (profilesGeneration.current === generation) setProfiles([]);
      return false;
    }
  }, []);

  const walletGeneration = useRef(0);
  const loadWallet = useCallback(async (): Promise<boolean> => {
    const generation = ++walletGeneration.current;
    try {
      const next = await listTicketWallet();
      if (walletGeneration.current === generation) {
        setWallet(next);
        setWalletError(null);
      }
      return true;
    } catch (e) {
      // Keep last good wallet; surface a visible degradation notice.
      if (walletGeneration.current === generation) setWalletError(e);
      return false;
    }
  }, []);

  useEffect(() => {
    if (poolState === 'idle') void poolEnsureLoaded();
  }, [poolState, poolEnsureLoaded]);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  useEffect(() => {
    void loadWallet();
  }, [loadWallet]);

  /** 生效 provider 命中 adapter 生成投影 → 「经兼容路由」徽标（profile 联结，不读 provider.meta） */
  const adapterBadgeHits = useMemo(() => {
    const hits = new Map<AgentId, { profile: AdapterProfile; sourceLabel?: string }>();
    if (profiles.length === 0) return hits;
    for (const meta of AGENTS) {
      const current = providersForAgent(pool.providers, meta.id).find((p) => p.isCurrent);
      if (!current) continue;
      const profile = profiles.find((p) => p.generatedProviderId === current.id);
      if (!profile) continue;
      const sourceLabel =
        profile.sourceKind === 'account'
          ? pool.accounts.find((a) => a.id === profile.sourceId)?.label
          : pool.providers.find((p) => p.id === profile.sourceId)?.name;
      hits.set(meta.id, { profile, sourceLabel });
    }
    return hits;
  }, [pool.accounts, pool.providers, profiles]);

  // 桥状态：仅对生效 provider 命中的 bridge 型 profile 轮询；查询失败显示「状态不可用」而非隐藏
  useEffect(() => {
    const bridgeProfiles = [...adapterBadgeHits.values()]
      .map((hit) => hit.profile)
      .filter((profile) => profile.route === 'local_bridge');
    if (bridgeProfiles.length === 0) {
      setBridgeStates({});
      return;
    }
    let cancelled = false;
    let timer: number | undefined;
    // 链式轮询：上一轮完成后再排下一轮，慢请求不产生重叠与陈旧覆盖
    const poll = async () => {
      const next: Record<string, AgentCardBridgeState> = {};
      await Promise.all(
        bridgeProfiles.map(async (profile) => {
          try {
            const status = await getAdapterBridgeStatus(profile.id);
            next[profile.id] = mapBridgeState(status.state);
          } catch {
            next[profile.id] = 'unavailable';
          }
        }),
      );
      if (cancelled) return;
      setBridgeStates(next);
      timer = window.setTimeout(() => void poll(), BRIDGE_POLL_MS);
    };
    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [adapterBadgeHits]);

  const badgeInputs = useMemo(() => {
    const inputs: Partial<Record<AgentId, AgentCardBadgeInput>> = {};
    for (const meta of AGENTS) {
      const hit = adapterBadgeHits.get(meta.id);
      const bridgeState =
        hit?.profile.route === 'local_bridge' ? bridgeStates[hit.profile.id] : undefined;
      const active = wallet ? activeBindingForAgent(wallet, meta.id) : null;
      const binding = active
        ? {
            ticketLabel: active.ticket.label,
            routeLabel: bindingRouteDashboardLabel(active.binding.route),
          }
        : null;
      if (!hit && !binding) continue;
      inputs[meta.id] = {
        ...(hit ? { viaAdapter: { sourceLabel: hit.sourceLabel } } : {}),
        ...(bridgeState ? { bridge: { state: bridgeState, profileId: hit?.profile.id ?? null } } : {}),
        ...(binding ? { binding } : {}),
      };
    }
    return inputs;
  }, [adapterBadgeHits, bridgeStates, wallet]);

  const handleConnectRequest = useCallback((agentId: AgentId) => {
    setConnectEntry({ mode: 'for-agent', targetAgentId: agentId });
  }, []);

  /** 快捷操作：打开卡片同款 for-agent ConnectFlow（新钱包页无 mode=providers 切换）。 */
  const openForAgentConnect = useCallback(() => {
    const installed = agents?.filter((item) => item.installed).map((item) => item.agentId) ?? [];
    const target =
      agentFilter !== 'all' && installed.includes(agentFilter)
        ? agentFilter
        : installed[0] ?? null;
    if (!target) {
      toast({ title: '请先安装 Agent', variant: 'danger' });
      return;
    }
    handleConnectRequest(target);
  }, [agents, agentFilter, handleConnectRequest, toast]);

  /** 回跳 `/?connect=`：agents 就绪后打开对应 ConnectFlow，并 replace 掉 query，避免关窗后重开。 */
  const consumedConnectRef = useRef<string | null>(null);
  useEffect(() => {
    const raw = searchParams.get('connect');
    if (raw == null || raw === '') {
      consumedConnectRef.current = null;
      return;
    }
    // 首次加载未完成：不要清掉 query，等已安装列表可用后再解析
    if (agents == null) return;
    if (consumedConnectRef.current === raw) return;

    const allowed = agents.filter((item) => item.installed).map((item) => item.agentId);
    const targetAgentId = parseConnectResumeParam(raw, allowed);
    consumedConnectRef.current = raw;
    if (targetAgentId) {
      setConnectEntry({ mode: 'for-agent', targetAgentId });
    }
    setSearchParams(consumeConnectResume(searchParams), { replace: true });
  }, [searchParams, setSearchParams, agents]);

  /**
   * 连接变更后重载页面数据；任一失败则抛出，由对话框呈现「已应用/已切换，但列表刷新失败」。
   * 注意：loadAgents/loadProfiles 内部消化异常（返回 boolean），pool.reload 对
   * partial/error 也正常 resolve——必须查返回值与 store 快照，不能依赖 reject。
   */
  const handleConnectionChanged = useCallback(async () => {
    const [agentsOk, profilesOk, walletOk] = await Promise.all([
      loadAgents(),
      loadProfiles(),
      loadWallet(),
    ]);
    await poolReload().catch(() => {});
    // 双侧刷新失败时 store 保留旧 state:'ready' 并写入 errors，必须一并检查
    const poolSnapshot = getConnectionPoolSnapshot();
    const poolOk =
      poolSnapshot.state === 'ready' && !poolSnapshot.errors.accounts && !poolSnapshot.errors.providers;
    if (!agentsOk || !profilesOk || !walletOk || !poolOk) {
      throw new Error('页面数据刷新失败，可手动刷新查看最新状态');
    }
  }, [loadAgents, poolReload, loadProfiles, loadWallet]);

  /** days / agentFilter 变化时各请求一次，上下共用 */
  const loadUsage = useCallback(
    async (initial: boolean) => {
      if (initial) setUsageLoading(true);
      else setUsageRefreshing(true);
      setUsageError(null);
      try {
        const availability = await getUsageAvailability();
        setUsageAvailability(availability);
        if (availability.status === 'unavailable') {
          setTrend([]);
          setUsage([]);
          setModels([]);
          return;
        }
        const [trendData, records, modelList] = await Promise.all([
          usageTrend(days, agentFilter),
          queryUsage({ days, agentId: agentFilter }),
          listModels().catch(() => [] as string[]),
        ]);
        setTrend(trendData);
        setUsage(records);
        setModels(modelList);
      } catch (e) {
        setUsageError(e);
      } finally {
        setUsageLoading(false);
        setUsageRefreshing(false);
      }
    },
    [days, agentFilter],
  );

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    void loadUsage(usage === null && usageAvailability === null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadUsage]);

  // /?section=usage 或 /usage 重定向后滚到用量段
  useEffect(() => {
    if (searchParams.get('section') !== 'usage') return;
    const t = window.setTimeout(() => {
      usageSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 80);
    return () => window.clearTimeout(t);
  }, [searchParams, agentsLoading, usageLoading]);

  const handleBackupAll = async () => {
    if (!agents) return;
    const installed = agents.filter((a) => a.installed);
    if (installed.length === 0) {
      toast({ title: '没有已安装的 agent', variant: 'danger' });
      return;
    }
    setBackingUp(true);
    try {
      await Promise.all(installed.map((a) => createBackup(a.agentId, 'Dashboard 手动备份')));
      toast({ title: '备份完成', description: `已为 ${installed.length} 个 agent 创建备份`, variant: 'success' });
    } catch (e) {
      toast({ title: '备份失败', description: String(e), variant: 'danger' });
    } finally {
      setBackingUp(false);
    }
  };

  const usageUnavailable = usageAvailability?.status === 'unavailable';
  const usageUnavailableReason =
    usageAvailability?.status === 'unavailable'
      ? usageAvailability.reason
      : 'Usage 尚未接入';

  const handleCollect = async () => {
    if (usageUnavailable) {
      toast({
        title: 'Usage 不可用',
        description: usageUnavailableReason,
        variant: 'danger',
      });
      return;
    }
    await usageSync.manualCollect();
  };

  // 手动/自动采集完成后刷新总览数据
  useEffect(() => {
    const onCollected = (_ev: Event) => {
      setHealthRefreshKey((k) => k + 1);
      void loadUsage(false);
    };
    window.addEventListener(USAGE_COLLECTED_EVENT, onCollected);
    return () => window.removeEventListener(USAGE_COLLECTED_EVENT, onCollected);
  }, [loadUsage]);

  /** 「今天」在 days=1 拉取后再按本地日历日收窄；其余范围直接用后端窗口 */
  const rangedUsage = useMemo(() => {
    const list = usage ?? [];
    if (dateRange !== 'today') return list;
    return list.filter((r) => isLocalToday(r.timestamp));
  }, [usage, dateRange]);

  const rangedTrend = useMemo(() => {
    if (dateRange !== 'today') return trend;
    const key = localDateKey();
    return trend.filter((p) => p.date === key);
  }, [trend, dateRange]);

  const metrics = useMemo(() => {
    const list = rangedUsage;
    // 输入 = 计费/non-cached（与 ccusage 一致）；缓存命中按 full prompt
    let billableIn = 0;
    let fullIn = 0;
    let output = 0;
    let cacheRead = 0;
    let cost = 0;
    for (const r of list) {
      const p = usageTokenParts(r);
      billableIn += p.billableInput;
      fullIn += p.fullInput;
      output += r.outputTokens;
      cacheRead += p.cache;
      cost += r.costUsd;
    }
    return {
      input: fmtTokens(billableIn),
      output: fmtTokens(output),
      cacheHit: fullIn > 0 ? `${Math.round((cacheRead / fullIn) * 100)}%` : '—',
      cost: `$${cost.toFixed(2)}`,
      totalIn: billableIn,
      totalOut: output,
      totalCost: cost,
    };
  }, [rangedUsage]);

  /** 分布:全部 agent 时按 agent 聚合;选中单个 agent 时按模型聚合 */
  const distribution = useMemo(() => {
    const list = rangedUsage;
    const byKey = new Map<string, { label: string; color: string; tokens: number; cost: number }>();
    for (const r of list) {
      const key = agentFilter === 'all' ? r.agentId : r.model;
      const meta = AGENT_MAP[r.agentId];
      const entry = byKey.get(key) ?? {
        label: agentFilter === 'all' ? meta.name : r.model,
        color: meta.color,
        tokens: 0,
        cost: 0,
      };
      const p = usageTokenParts(r);
      // total tokens ≈ billable input + cache + output (ccusage totalTokens)
      entry.tokens += p.billableInput + p.cache + r.outputTokens;
      entry.cost += r.costUsd;
      byKey.set(key, entry);
    }
    return [...byKey.values()].sort((a, b) => b.tokens - a.tokens);
  }, [rangedUsage, agentFilter]);

  /** 模型筛选仅作用于明细表，不改 metrics / trend */
  const tableRows = useMemo(() => {
    const filtered =
      modelFilter === 'all'
        ? rangedUsage
        : rangedUsage.filter((r) => r.model === modelFilter);
    return [...filtered].sort((a, b) => b.timestamp.localeCompare(a.timestamp));
  }, [rangedUsage, modelFilter]);

  const trendAgents = agentFilter === 'all' ? AGENTS : [AGENT_MAP[agentFilter]];
  const maxTokens = distribution[0]?.tokens ?? 0;
  const installedCount = agents?.filter((a) => a.installed).length ?? 0;
  const envBad = hasEnvIssues(runtimes);
  const showEnvCta = !agentsLoading && agents !== null && installedCount === 0 && envBad;

  return (
    <div>
      <PageHeader
        title="总览"
        description="状态与用量"
        descriptionTip="上半为各 Agent 状态，下半为本地日志解析的 Token 用量与成本估算。"
      />

      {/* —— 上半：Agent 总览（独立 loading / error）—— */}
      <PageSection first>
        {agentsLoading ? (
          <AgentOverviewSkeleton />
        ) : agentsError ? (
          <ErrorState error={agentsError} onRetry={() => void loadAgents()} />
        ) : agents ? (
          <div className={showEnvCta ? pageRhythm.lead : undefined}>
            {showEnvCta && (
              <Notice
                className="text-sm"
                tone="warning"
                actionLabel="去修复"
                onAction={() => navigate('/agents')}
              >
                <p className="font-medium text-warning">环境未就绪，尚未安装 Agent</p>
                <p className="mt-0.5 text-secondary">先修运行环境，再装 CLI</p>
              </Notice>
            )}
            <AgentOverview
              agents={agents}
              onConnectRequest={handleConnectRequest}
              badgeInputs={badgeInputs}
            />
            {walletError ? (
              <Notice
                className="mt-3 text-sm"
                tone="warning"
                actionLabel="重试"
                onAction={() => void loadWallet()}
              >
                票钱包刷新失败，卡片绑定信息可能不是最新。
              </Notice>
            ) : null}
          </div>
        ) : null}
      </PageSection>

      {/* —— 用量总览：筛选 + 指标 + 趋势 + 分布 —— */}
      <PageSection>
        <div className={cn(pageRhythm.chromeRow)}>
          <Select value={agentFilter} onValueChange={(v) => setAgentFilter(v as AgentId | 'all')}>
            <SelectTrigger className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全部 Agent</SelectItem>
              {AGENTS.map((a) => (
                <SelectItem key={a.id} value={a.id}>
                  {a.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={modelFilter} onValueChange={setModelFilter}>
            <SelectTrigger className="w-44">
              <SelectValue placeholder="全部模型" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全部模型</SelectItem>
              {models.map((m) => (
                <SelectItem key={m} value={m}>
                  {m}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={dateRange} onValueChange={(v) => setDateRange(v as DateRange)}>
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DATE_RANGE_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className="ml-auto flex min-w-[12.5rem] max-w-full flex-col items-end gap-1.5 sm:min-w-[16rem]">
            <div className="flex flex-wrap items-center justify-end gap-2">
              {!collecting && (
                <Tip
                  className="max-w-[12rem] shrink-0 text-right text-xs leading-snug text-muted sm:max-w-[14rem]"
                  label={usageSync.statusLine}
                >
                  {usageSync.statusLine}
                </Tip>
              )}
              <Button
                size="sm"
                variant="outline"
                className="shrink-0"
                onClick={() => void handleCollect()}
                disabled={collecting || usageUnavailable || usageLoading}
                title={
                  usageUnavailable
                    ? usageUnavailableReason
                    : usageSync.intervalMin > 0
                      ? `从本地日志增量导入；也可等自动同步（每 ${usageSync.intervalMin} 分钟，仅前台）`
                      : '从本地日志增量导入；当前仅手动（间隔为 0）'
                }
              >
                <RefreshCw className={collecting ? 'h-3.5 w-3.5 animate-spin' : 'h-3.5 w-3.5'} />
                {collecting
                  ? usageSync.collectSource === 'auto'
                    ? '同步中…'
                    : '采集中…'
                  : '采集'}
              </Button>
            </div>
            {collecting && (
              <div className="flex w-full max-w-[16rem] items-center gap-2">
                <Progress value={collectPct} className="h-1.5 flex-1" />
                <span className="shrink-0 tabular-nums text-xs text-secondary">{collectPct}%</span>
              </div>
            )}
          </div>
        </div>

        {usageLoading ? (
          <UsageOverviewSkeleton />
        ) : usageUnavailable ? (
          <Card className="p-6">
            <div className="flex flex-col items-start gap-2">
              <p className="text-sm font-medium text-primary">用量不可用</p>
              <p className="text-xs text-secondary">{usageUnavailableReason}</p>
              <p className="text-xs text-muted">
                演示数据请用 <code className="font-mono">pnpm dev:mock</code>
              </p>
            </div>
          </Card>
        ) : usageError ? (
          <ErrorState error={usageError} onRetry={() => void loadUsage(true)} />
        ) : (
          <div
            className={cn(
              pageRhythm.blocks,
              usageRefreshing ? 'opacity-60 transition-opacity' : 'transition-opacity',
            )}
          >
            <div className={pageRhythm.metricGrid}>
              <MetricCard label={`输入(${dayLabel})`} value={metrics.input} />
              <MetricCard label={`输出(${dayLabel})`} value={metrics.output} />
              <MetricCard label="缓存命中" value={metrics.cacheHit} />
              <MetricCard label="估算成本" value={metrics.cost} />
            </div>

            <div className="grid grid-cols-3 items-start gap-4">
              <Card className="col-span-2">
                <CardHeader>
                  <CardTitle>{dayLabel} Token 用量</CardTitle>
                  <p className="text-xs text-muted">
                    合计 {fmtTokens(metrics.totalIn)} in / {fmtTokens(metrics.totalOut)} out / ≈$
                    {metrics.totalCost.toFixed(1)}
                  </p>
                </CardHeader>
                <CardContent>
                  <div className="h-56">
                    <ResponsiveContainer width="100%" height="100%">
                      <AreaChart data={rangedTrend} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
                        <defs>
                          {trendAgents.map((meta) => (
                            <linearGradient
                              key={`grad-${meta.id}`}
                              id={`usage-fill-${meta.id}`}
                              x1="0"
                              y1="0"
                              x2="0"
                              y2="1"
                            >
                              <stop offset="0%" stopColor={meta.color} stopOpacity={0.18} />
                              <stop offset="100%" stopColor={meta.color} stopOpacity={0.02} />
                            </linearGradient>
                          ))}
                        </defs>
                        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} strokeOpacity={0.6} />
                        <XAxis
                          dataKey="date"
                          tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                          tickLine={false}
                          axisLine={{ stroke: 'var(--border)' }}
                          tickFormatter={(d: string) => d.slice(5)}
                        />
                        <YAxis
                          tick={{ fill: 'var(--text-muted)', fontSize: 11 }}
                          tickLine={false}
                          axisLine={false}
                          tickFormatter={(v: number) => fmtTokens(v)}
                          width={48}
                        />
                        <Tooltip
                          contentStyle={{
                            backgroundColor: 'var(--bg-panel)',
                            border: '1px solid var(--border)',
                            borderRadius: 8,
                            fontSize: 12,
                          }}
                          labelStyle={{ color: 'var(--text-secondary)' }}
                          formatter={(value, name) => [
                            fmtTokens(Number(value)),
                            agentDisplayName(name as AgentId),
                          ]}
                        />
                        {trendAgents.map((meta) => (
                          <Area
                            key={meta.id}
                            type="monotone"
                            dataKey={meta.id}
                            stackId="total"
                            stroke={meta.color}
                            strokeWidth={1.5}
                            fill={`url(#usage-fill-${meta.id})`}
                            activeDot={{ r: 3, strokeWidth: 0 }}
                          />
                        ))}
                      </AreaChart>
                    </ResponsiveContainer>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>快捷操作</CardTitle>
                </CardHeader>
                <CardContent className="flex flex-col gap-2">
                  <Button
                    variant="outline"
                    className="justify-start"
                    onClick={openForAgentConnect}
                  >
                    <ArrowLeftRight className="h-4 w-4" /> 切换供应商
                  </Button>
                  <Button
                    variant="outline"
                    className="justify-start"
                    onClick={openForAgentConnect}
                  >
                    <UserRound className="h-4 w-4" /> 切换账号
                  </Button>
                  <Button
                    variant="outline"
                    className="justify-start"
                    disabled={backingUp || !agents}
                    onClick={() => void handleBackupAll()}
                  >
                    <DatabaseBackup className="h-4 w-4" />
                    {backingUp ? '备份中…' : '立即备份'}
                  </Button>
                </CardContent>
              </Card>
            </div>

            <Card>
              <CardHeader>
                <CardTitle>{agentFilter === 'all' ? 'Agent 用量分布' : '模型用量分布'}</CardTitle>
              </CardHeader>
              <CardContent>
                {distribution.length === 0 ? (
                  <p className="py-4 text-sm text-secondary">暂无数据</p>
                ) : (
                  <ul className="space-y-1.5">
                    {distribution.map((d) => (
                      <li key={d.label} className="flex h-7 items-center gap-3">
                        <span className="flex min-w-0 flex-1 items-center gap-1.5 truncate text-sm">
                          <AgentDot color={d.color} size="md" title={null} />
                          <span className="truncate">{d.label}</span>
                        </span>
                        {d.tokens === 0 ? (
                          <span className="shrink-0 text-xs text-muted">无数据</span>
                        ) : (
                          <>
                            <div className="h-1.5 w-32 shrink-0 overflow-hidden rounded-full bg-subtle sm:w-40">
                              <div
                                className="h-full rounded-full"
                                style={{
                                  width: maxTokens > 0 ? `${(d.tokens / maxTokens) * 100}%` : 0,
                                  backgroundColor: d.color,
                                }}
                              />
                            </div>
                            <span className="w-20 shrink-0 text-right font-mono text-xs text-secondary">
                              {fmtTokens(d.tokens)}
                            </span>
                            <span className="w-14 shrink-0 text-right font-mono text-xs text-muted">
                              ${d.cost.toFixed(2)}
                            </span>
                          </>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          </div>
        )}
      </PageSection>

      {/* —— 用量明细（大段分割）—— */}
      <PageSection
        ref={usageSectionRef}
        id="usage"
        ruled
        title="用量明细"
        description="与上方共用时间、Agent 筛选；模型筛选仅作用于本表。"
      >
        {showGuide && !usageUnavailable && (
          <Notice
            className="mb-4"
            tone="info"
            onDismiss={() => {
              setShowGuide(false);
              saveBool(StorageKey.usageGuideDismissed, true);
            }}
          >
            首次请点「采集」导入历史；之后可按设置自动同步（仅前台）。
          </Notice>
        )}

        {usageLoading ? (
          <TableSkeleton rows={8} cols={8} />
        ) : usageUnavailable ? (
          <EmptyState
            icon={BarChart3}
            title="用量不可用"
            description={usageUnavailableReason}
          />
        ) : usageError ? (
          <ErrorState
            compact
            error={usageError}
            onRetry={() => void loadUsage(true)}
            title="用量加载失败"
          />
        ) : tableRows.length === 0 ? (
          <EmptyState
            icon={BarChart3}
            title="暂无用量"
            description="调整筛选，或点「采集」同步本地日志"
            actionLabel="采集"
            onAction={() => void handleCollect()}
          />
        ) : (
          <UsageDetailsTable rows={tableRows} />
        )}

        {!usageUnavailable && (
          <UsageParserHealth variant="dashboard" refreshKey={healthRefreshKey} />
        )}
      </PageSection>

      <ConnectFlowDialog
        entry={connectEntry}
        deps={connectDeps}
        onClose={() => setConnectEntry(null)}
        onConnectionChanged={handleConnectionChanged}
        onNavigate={(to) => navigate(to)}
      />
    </div>
  );
}

function MetricCard({ label, value }: { label: string; value: string }) {
  return (
    <Card className="p-3">
      <p className="text-xs text-muted">{label}</p>
      <p className="mt-1 text-xl font-semibold tracking-tight">{value}</p>
    </Card>
  );
}

function UsageOverviewSkeleton() {
  return (
    <div className={pageRhythm.blocks}>
      <div className={pageRhythm.metricGrid}>
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-20" />
        ))}
      </div>
      <div className="grid grid-cols-3 gap-4">
        <Card className="col-span-2 p-4">
          <Skeleton className="h-4 w-32" />
          <Skeleton className="mt-4 h-56 w-full" />
        </Card>
        <Card className="space-y-2 p-4">
          <Skeleton className="h-4 w-20" />
          <Skeleton className="h-9 w-full" />
          <Skeleton className="h-9 w-full" />
          <Skeleton className="h-9 w-full" />
        </Card>
      </div>
      <Card className="space-y-3 p-4">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-full" />
      </Card>
    </div>
  );
}
