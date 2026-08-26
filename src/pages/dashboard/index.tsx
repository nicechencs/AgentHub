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
  BarChart3,
  RefreshCw,
} from 'lucide-react';

import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useTheme } from '@/components/shared/ThemeProvider';
import { Notice } from '@/components/shared/Notice';
import { UsageParserHealth } from '@/components/shared/UsageParserHealth';
import { useUsageSync } from '@/components/shared/UsageSyncProvider';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Tip, tooltipSurfaceStyle } from '@/components/ui/tooltip';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton, TableSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';

import {
  filterVisibleTrend,
  visibleInstalledIds,
} from '@/lib/agent-visibility';
import {
  getAdapterBridgeStatus,
  listAdapterProfiles,
  type AdapterBridgeRuntimeState,
  type AdapterProfile,
} from '@/lib/api/adapter';
import { listRuntimes } from '@/lib/api/env';
import {
  getUsageAvailability,
  queryUsage,
  usageOverview as fetchUsageOverview,
  usageTrend,
  type UsageAvailability,
  type UsageOverview,
} from '@/lib/api/usage';

import type { MessageKey } from '@/lib/i18n';
import { activeBindingForAgent } from '@/lib/ticket-wallet';
import { ConnectFlowDialog } from '@/components/connect/ConnectFlowDialog';
import { consumeConnectResume, parseConnectResumeParam } from '@/lib/connect-flow/connect-intent';
import { createDefaultConnectFlowDeps } from '@/lib/connect-flow/default-deps';
import type { ConnectFlowEntry } from '@/lib/connect-flow/types';
import {
  getConnectionPoolSnapshot,
  getTicketWalletSnapshot,
  providersForAgent,
  useAgentCatalogOptional,
  useConnectionPool,
  useTicketWallet,
} from '@/app/runtime';
import { AGENTS, AGENT_MAP, agentDisplayName } from '@/config/agents';
import { hasEnvIssues } from '@/lib/env';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';
import { resolveTheme } from '@/lib/theme';
import type { AgentId, RuntimeDetect, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { resolveChartColor, typeScalePx } from '@/styles/tokens';
import { USAGE_COLLECTED_EVENT } from '@/lib/usage-sync';
import {
  formatTrendTick,
  formatTrendTooltipLabel,
  zeroFillTrendSeries,
} from '@/lib/usage-trend';
import { cn, fmtTokens } from '@/lib/utils';
import { AgentOverview, AgentOverviewSkeleton } from './AgentOverview';
import {
  dashboardOverviewSkeletonCount,
  dashboardPageDescription,
  installedOverviewScope,
  summarizeAgentOverview,
  type AgentCardBadgeInput,
  type AgentCardBridgeState,
} from './agentOverviewModel';
import { UsageDetailsTable } from './UsageDetailsTable';
import { isLatestUsageRequest } from './usage-request';
import {
  coerceModelFilter,
  decorateUsageDistribution,
  filterByAgent,
  filterByModel,
  filterHiddenUsageOverview,
  filterWindowUsage,
  overviewToUsageMetrics,
  sortUsageRowsDesc,
  usageWindowBound,
  type DateRange,
} from './usageOverviewModel';

const DATE_RANGE_OPTIONS: { value: DateRange; days: number }[] = [
  { value: 'today', days: 1 },
  { value: '24h', days: 1 },
  { value: '7d', days: 7 },
  { value: '30d', days: 30 },
];

const DATE_RANGE_LABEL_KEYS: Record<DateRange, MessageKey> = {
  today: 'dashboard.range.today',
  '24h': 'dashboard.range.last24h',
  '7d': 'dashboard.range.last7d',
  '30d': 'dashboard.range.last30d',
};

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
  const { t } = useI18n();
  const { theme } = useTheme();
  const chartScheme = resolveTheme(theme);
  const usageSync = useUsageSync();
  const usageSectionRef = useRef<HTMLElement>(null);
  const {
    state: agentState,
    statuses,
    error: agentsError,
    reload: reloadAgentStatuses,
    installedIds,
    installedAgents,
    omittedIds,
  } = useInstalledAgents();
  const catalog = useAgentCatalogOptional();

  // Detect snapshot is shared; cards render as soon as doctor is ready,
  // even while live-auth is still refreshing.
  const agents = statuses;
  const agentsLoading = agentState === 'idle' || agentState === 'loading';
  const showAgentError = agentState === 'error' && statuses.length === 0;

  const [runtimes, setRuntimes] = useState<RuntimeDetect[]>([]);

  // —— 页面级共享筛选（时间 + Agent + 模型；指标 / 趋势 / 分布 / 明细共用）——
  const [dateRange, setDateRange] = useState<DateRange>('7d');
  const [agentFilter, setAgentFilter] = useState<AgentId | 'all'>('all');
  const [modelFilter, setModelFilter] = useState<string>('all');

  // —— 用量：overview/trend 先画图；明细表另拉 capped 页 ——
  const [usageAvailability, setUsageAvailability] = useState<UsageAvailability | null>(null);
  const [usageOverview, setUsageOverview] = useState<UsageOverview | null>(null);
  const [usageTrendPoints, setUsageTrendPoints] = useState<UsageTrendPoint[]>([]);
  const [usage, setUsage] = useState<UsageRecord[] | null>(null);
  const [usageLoading, setUsageLoading] = useState(true);
  const [tableLoading, setTableLoading] = useState(true);
  const [usageRefreshing, setUsageRefreshing] = useState(false);
  const [usageError, setUsageError] = useState<unknown>(null);
  const usageGenerationRef = useRef(0);

  const dayLabel = t(DATE_RANGE_LABEL_KEYS[dateRange]);

  // —— 采集（状态由 UsageSyncProvider 统一管理）——
  const collecting = usageSync.collecting;
  const collectPct = usageSync.collectPct;
  const [healthRefreshKey, setHealthRefreshKey] = useState(0);
  const [showGuide, setShowGuide] = useState(() => !loadBool(StorageKey.usageGuideDismissed));

  const loadRuntimes = useCallback(async (): Promise<boolean> => {
    try {
      setRuntimes(await listRuntimes());
      return true;
    } catch {
      return false;
    }
  }, []);

  /** 返回是否成功：连接流程的刷新契约需要真实成败，不能吞掉失败。 */
  const loadAgents = useCallback(async (): Promise<boolean> => {
    try {
      const [agentsOk, runtimesOk] = await Promise.all([
        reloadAgentStatuses().then(() => true, () => false),
        loadRuntimes(),
      ]);
      return agentsOk && runtimesOk;
    } catch {
      return false;
    }
  }, [loadRuntimes, reloadAgentStatuses]);

  // —— 连接流程：卡片徽标数据 + `/?connect=` 回跳打开 ConnectFlowDialog ——
  const pool = useConnectionPool();
  const {
    wallet,
    error: walletError,
    state: walletState,
    reload: walletReload,
    ensureLoaded: walletEnsureLoaded,
  } = useTicketWallet();
  const [profiles, setProfiles] = useState<AdapterProfile[]>([]);
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

  const loadWallet = useCallback(async (): Promise<boolean> => {
    try {
      await walletReload();
      const snap = getTicketWalletSnapshot();
      return snap.wallet != null && snap.error == null;
    } catch {
      return false;
    }
  }, [walletReload]);

  useEffect(() => {
    if (poolState === 'idle') void poolEnsureLoaded();
  }, [poolState, poolEnsureLoaded]);

  useEffect(() => {
    if (walletState === 'idle') void walletEnsureLoaded();
  }, [walletEnsureLoaded, walletState]);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  useEffect(() => {
    void loadRuntimes();
  }, [loadRuntimes]);

  /** 生效 provider 命中 adapter 生成投影 → 「本机路由」徽标（profile 联结，不读 provider.meta） */
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
            routeLabel:
              active.binding.route === 'reshape'
                ? t('connections.list.routeReshape')
                : active.binding.route === 'bridge'
                  ? t('kind.route.localRoute')
                  : t('kind.route.direct'),
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
  }, [adapterBadgeHits, bridgeStates, wallet, t]);

  /** 回跳 `/?connect=`：agents 就绪后打开对应 ConnectFlow，并 replace 掉 query，避免关窗后重开。 */
  const consumedConnectRef = useRef<string | null>(null);
  useEffect(() => {
    const raw = searchParams.get('connect');
    if (raw == null || raw === '') {
      consumedConnectRef.current = null;
      return;
    }
    // 首次加载未完成：不要清掉 query，等已安装列表可用后再解析
    if (agentsLoading) return;
    if (consumedConnectRef.current === raw) return;

    const allowed = agents.filter((item) => item.installed && !item.hidden).map((item) => item.agentId);
    const targetAgentId = parseConnectResumeParam(raw, allowed);
    consumedConnectRef.current = raw;
    if (targetAgentId) {
      setConnectEntry({ mode: 'for-agent', targetAgentId });
    }
    setSearchParams(consumeConnectResume(searchParams), { replace: true });
  }, [searchParams, setSearchParams, agents, agentsLoading]);

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
      throw new Error(t('dashboard.page.refreshFailed'));
    }
  }, [loadAgents, poolReload, loadProfiles, loadWallet, t]);

  /** overview+trend 先画图；明细表另拉 capped 页。筛选变化走后端。 */
  const loadUsage = useCallback(
    async (initial: boolean) => {
      const generation = ++usageGenerationRef.current;
      if (initial) setUsageLoading(true);
      else setUsageRefreshing(true);
      setTableLoading(true);
      setUsageError(null);
      const { days, since } = usageWindowBound(dateRange);
      const agentId = agentFilter === 'all' ? undefined : agentFilter;
      const model = modelFilter === 'all' || modelFilter === '' ? undefined : modelFilter;
      const excludeAgentIds = omittedIds.length > 0 ? omittedIds : undefined;
      try {
        const [availability, overview, trend] = await Promise.all([
          getUsageAvailability(),
          fetchUsageOverview({ days, agentId, model, since, excludeAgentIds }),
          usageTrend(days, agentId, model, since, excludeAgentIds),
        ]);
        if (!isLatestUsageRequest(usageGenerationRef.current, generation)) return;
        setUsageAvailability(availability);
        if (availability.status === 'unavailable') {
          setUsageOverview(null);
          setUsageTrendPoints([]);
          setUsage([]);
          setTableLoading(false);
          return;
        }
        setUsageOverview(overview);
        setUsageTrendPoints(trend);
        setUsageLoading(false);
        setUsageRefreshing(false);

        try {
          const records = await queryUsage({
            days,
            agentId,
            model,
            since,
            limit: 2000,
            excludeAgentIds,
          });
          if (!isLatestUsageRequest(usageGenerationRef.current, generation)) return;
          setUsage(records);
        } catch {
          if (isLatestUsageRequest(usageGenerationRef.current, generation)) setUsage([]);
        } finally {
          if (isLatestUsageRequest(usageGenerationRef.current, generation)) setTableLoading(false);
        }
      } catch (e) {
        if (isLatestUsageRequest(usageGenerationRef.current, generation)) setUsageError(e);
      } finally {
        if (isLatestUsageRequest(usageGenerationRef.current, generation)) {
          setUsageLoading(false);
          setUsageRefreshing(false);
          setTableLoading(false);
        }
      }
    },
    [dateRange, agentFilter, modelFilter, omittedIds],
  );

  useEffect(() => {
    void loadUsage(usage === null && usageAvailability === null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadUsage]);

  // /?section=usage 或 /usage 重定向后滚到用量段
  useEffect(() => {
    if (searchParams.get('section') !== 'usage') return;
    const scrollTimer = window.setTimeout(() => {
      usageSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 80);
    return () => window.clearTimeout(scrollTimer);
  }, [searchParams, agentsLoading, usageLoading]);

  const usageUnavailable = usageAvailability?.status === 'unavailable';
  const usageUnavailableReason =
    usageAvailability?.status === 'unavailable'
      ? usageAvailability.reason
      : t('dashboard.page.usageNotWired');

  const handleCollect = async () => {
    if (usageUnavailable) {
      toast({
        title: t('dashboard.page.usageUnavailableTitle'),
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

  const parseVisibleIds = useMemo(
    () => (agentsLoading ? undefined : visibleInstalledIds(agents)),
    [agents, agentsLoading],
  );

  useEffect(() => {
    if (agentFilter === 'all' || agentsLoading) return;
    if (!installedIds.includes(agentFilter)) {
      setAgentFilter('all');
    }
  }, [agentFilter, agentsLoading, installedIds]);

  const groupedByAgent = agentFilter === 'all';
  const visibleOverview = useMemo(() => {
    if (!usageOverview) return null;
    return filterHiddenUsageOverview(usageOverview, omittedIds, groupedByAgent);
  }, [usageOverview, omittedIds, groupedByAgent]);

  const modelOptions = visibleOverview?.models ?? [];
  const effectiveModelFilter = coerceModelFilter(modelFilter, modelOptions);

  useEffect(() => {
    if (modelFilter !== effectiveModelFilter) {
      setModelFilter(effectiveModelFilter);
    }
  }, [modelFilter, effectiveModelFilter]);

  const rangedTrend = useMemo(() => {
    const visible = filterVisibleTrend(usageTrendPoints, omittedIds);
    const ids =
      agentFilter === 'all' ? installedAgents.map((meta) => meta.id) : [agentFilter];
    return zeroFillTrendSeries(visible, ids);
  }, [usageTrendPoints, omittedIds, agentFilter, installedAgents]);

  const metrics = useMemo(() => {
    const m = overviewToUsageMetrics(
      visibleOverview?.metrics ?? {
        billableInput: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        costUsd: 0,
      },
    );
    return {
      input: fmtTokens(m.billableInput),
      output: fmtTokens(m.output),
      cacheWrite: fmtTokens(m.cacheWrite),
      cacheRead: fmtTokens(m.cacheRead),
      cost: `$${m.cost.toFixed(2)}`,
      totalIn: m.billableInput,
      totalOut: m.output,
      totalCost: m.cost,
    };
  }, [visibleOverview]);

  const distribution = useMemo(
    () =>
      decorateUsageDistribution(visibleOverview?.distribution ?? [], agentFilter, AGENT_MAP),
    [visibleOverview, agentFilter],
  );

  /** 明细表仍用 capped rows + 既有客户端过滤（隐藏/未安装 Agent / 分页） */
  const windowUsage = useMemo(
    () => filterWindowUsage(usage ?? [], dateRange, omittedIds),
    [usage, dateRange, omittedIds],
  );
  const agentScopedUsage = useMemo(
    () => filterByAgent(windowUsage, agentFilter),
    [windowUsage, agentFilter],
  );
  const scopedUsage = useMemo(
    () => filterByModel(agentScopedUsage, effectiveModelFilter),
    [agentScopedUsage, effectiveModelFilter],
  );
  const tableRows = useMemo(() => sortUsageRowsDesc(scopedUsage), [scopedUsage]);

  const trendAgents = useMemo(() => {
    if (agentFilter !== 'all') {
      const meta = AGENT_MAP[agentFilter];
      return meta ? [meta] : [];
    }
    return installedAgents;
  }, [agentFilter, installedAgents]);
  const maxTokens = distribution[0]?.tokens ?? 0;
  const installedCount = agents?.filter((a) => a.installed && !a.hidden).length ?? 0;
  const overviewSkeletonCount = dashboardOverviewSkeletonCount(
    agents,
    installedIds.length,
  );
  const pageDescription = useMemo(() => {
    if (agentsLoading) return dashboardPageDescription(null, t);
    const { metas, statuses } = installedOverviewScope(
      catalog.hydrated ? AGENTS : [],
      agents,
    );
    return dashboardPageDescription(summarizeAgentOverview(metas, statuses, t), t);
  }, [agents, agentsLoading, catalog.hydrated, t]);
  const envBad = hasEnvIssues(runtimes);
  const showEnvCta = !agentsLoading && !showAgentError && installedCount === 0 && envBad;

  return (
    <div>
      <PageHeader
        title={t('dashboard.page.title')}
        description={pageDescription}
        descriptionTip={t('dashboard.page.descriptionTip')}
      />

      {/* —— 上半：Agent 总览（独立 loading / error）—— */}
      <PageSection first>
        {agentsLoading ? (
          <AgentOverviewSkeleton count={overviewSkeletonCount} />
        ) : showAgentError ? (
          <ErrorState error={agentsError} onRetry={() => void loadAgents()} />
        ) : (
          <div className={showEnvCta ? pageRhythm.lead : undefined}>
            {showEnvCta && (
              <Notice
                className="text-sm"
                tone="warning"
                actionLabel={t('dashboard.page.envNoticeAction')}
                onAction={() => navigate('/agents')}
              >
                <p className="font-medium text-warning">{t('dashboard.page.envNoticeTitle')}</p>
                <p className="mt-0.5 text-secondary">{t('dashboard.page.envNoticeBody')}</p>
              </Notice>
            )}
            <AgentOverview
              agents={agents}
              badgeInputs={badgeInputs}
            />
            {walletError ? (
              <Notice
                className="mt-3 text-sm"
                tone="warning"
                actionLabel={t('chrome.error.retry')}
                onAction={() => void loadWallet()}
              >
                {t('dashboard.page.walletRefreshFailed')}
              </Notice>
            ) : null}
          </div>
        )}
      </PageSection>

      {/* —— 用量总览：筛选 + 指标 + 趋势 + 分布 —— */}
      <PageSection>
        <div className={cn(pageRhythm.chromeRow)}>
          <Select value={agentFilter} onValueChange={(v) => setAgentFilter(v as AgentId | 'all')}>
            <SelectTrigger className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('dashboard.page.allAgents')}</SelectItem>
              {installedAgents.map((a) => (
                <SelectItem key={a.id} value={a.id}>
                  {a.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={effectiveModelFilter} onValueChange={setModelFilter}>
            <SelectTrigger className="w-44">
              <SelectValue placeholder={t('dashboard.page.allModels')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('dashboard.page.allModels')}</SelectItem>
              {modelOptions.map((m) => (
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
                  {t(DATE_RANGE_LABEL_KEYS[o.value])}
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
                      ? t('dashboard.page.collectTitleAuto', { minutes: usageSync.intervalMin })
                      : t('dashboard.page.collectTitleManual')
                }
              >
                <RefreshCw className={collecting ? 'h-3.5 w-3.5 animate-spin' : 'h-3.5 w-3.5'} />
                {collecting
                  ? usageSync.collectSource === 'auto'
                    ? t('dashboard.page.syncing')
                    : t('dashboard.page.collecting')
                  : t('dashboard.page.collect')}
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
              <p className="text-sm font-medium text-primary">{t('dashboard.page.usageUnavailable')}</p>
              <p className="text-xs text-secondary">{usageUnavailableReason}</p>
              <p className="text-xs text-muted">
                {t('dashboard.page.usageUnavailableDemo')}
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
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
              <MetricCard
                label={t('dashboard.page.metricInput', { range: dayLabel })}
                value={metrics.input}
              />
              <MetricCard
                label={t('dashboard.page.metricOutput', { range: dayLabel })}
                value={metrics.output}
              />
              <MetricCard
                label={t('dashboard.page.metricCacheWrite', { range: dayLabel })}
                value={metrics.cacheWrite}
              />
              <MetricCard
                label={t('dashboard.page.metricCacheRead', { range: dayLabel })}
                value={metrics.cacheRead}
              />
              <MetricCard label={t('dashboard.page.metricCost')} value={metrics.cost} />
            </div>

            <Card>
              <CardHeader>
                <CardTitle>
                  {t('dashboard.page.tokenUsageTitle', { range: dayLabel })}
                </CardTitle>
                <p className="text-xs text-muted">
                  {t('dashboard.page.tokenUsageSummary', {
                    in: fmtTokens(metrics.totalIn),
                    out: fmtTokens(metrics.totalOut),
                    cost: metrics.totalCost.toFixed(1),
                  })}
                </p>
              </CardHeader>
              <CardContent>
                <div className="h-56">
                    <ResponsiveContainer width="100%" height="100%">
                      <AreaChart data={rangedTrend} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
                        <defs>
                          {trendAgents.map((meta) => {
                            const color = resolveChartColor(meta.color, chartScheme);
                            return (
                              <linearGradient
                                key={`grad-${meta.id}`}
                                id={`usage-fill-${meta.id}`}
                                x1="0"
                                y1="0"
                                x2="0"
                                y2="1"
                              >
                                <stop offset="0%" stopColor={color} stopOpacity={0.18} />
                                <stop offset="100%" stopColor={color} stopOpacity={0.02} />
                              </linearGradient>
                            );
                          })}
                        </defs>
                        <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} strokeOpacity={0.6} />
                        <XAxis
                          dataKey="date"
                          tick={{ fill: 'var(--text-muted)', fontSize: typeScalePx('meta') }}
                          tickLine={false}
                          axisLine={{ stroke: 'var(--border)' }}
                          minTickGap={28}
                          interval="preserveStartEnd"
                          tickFormatter={(d: string) => formatTrendTick(d)}
                        />
                        <YAxis
                          tick={{ fill: 'var(--text-muted)', fontSize: typeScalePx('meta') }}
                          tickLine={false}
                          axisLine={false}
                          tickFormatter={(v: number) => fmtTokens(v)}
                          width={48}
                        />
                        <Tooltip
                          contentStyle={tooltipSurfaceStyle()}
                          labelStyle={{
                            color: 'var(--text-secondary)',
                            fontSize: 'var(--font-meta-size)',
                          }}
                          itemStyle={{
                            fontSize: 'var(--font-meta-size)',
                          }}
                          labelFormatter={(label) => formatTrendTooltipLabel(String(label))}
                          formatter={(value, name) => {
                            const tokens = Number(value);
                            if (!tokens) return null;
                            return [fmtTokens(tokens), agentDisplayName(name as AgentId)];
                          }}
                        />
                        {trendAgents.map((meta) => (
                          <Area
                            key={meta.id}
                            type="monotone"
                            dataKey={meta.id}
                            stroke={resolveChartColor(meta.color, chartScheme)}
                            strokeWidth={1.5}
                            fill={`url(#usage-fill-${meta.id})`}
                            isAnimationActive={false}
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
                <CardTitle>
                  {agentFilter === 'all'
                    ? t('dashboard.page.distByAgent')
                    : t('dashboard.page.distByModel')}
                </CardTitle>
              </CardHeader>
              <CardContent>
                {distribution.length === 0 ? (
                  <p className="py-4 text-sm text-secondary">{t('dashboard.page.noData')}</p>
                ) : (
                  <ul className="space-y-1.5">
                    {distribution.map((d) => (
                      <li key={d.key} className="flex h-7 items-center gap-3">
                        <span className="flex min-w-0 flex-1 items-center gap-1.5 truncate text-sm">
                          <AgentDot color={d.color} size="md" title={null} />
                          <span className="truncate">{d.label}</span>
                        </span>
                        {d.tokens === 0 ? (
                          <span className="shrink-0 text-xs text-muted">
                            {t('dashboard.page.noDataShort')}
                          </span>
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
        title={t('dashboard.page.detailsTitle')}
        description={t('dashboard.page.detailsDescription')}
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
            {t('dashboard.page.guide')}
          </Notice>
        )}

        {tableLoading ? (
          <TableSkeleton rows={8} cols={8} />
        ) : usageUnavailable ? (
          <EmptyState
            icon={BarChart3}
            title={t('dashboard.page.usageUnavailable')}
            description={usageUnavailableReason}
          />
        ) : usageError ? (
          <ErrorState
            compact
            error={usageError}
            onRetry={() => void loadUsage(true)}
            title={t('dashboard.page.usageLoadFailed')}
          />
        ) : tableRows.length === 0 ? (
          <EmptyState
            icon={BarChart3}
            title={t('dashboard.page.noUsage')}
            description={t('dashboard.page.noUsageDesc')}
            action={
              <Button size="sm" variant="outline" className="mt-2" onClick={() => void handleCollect()}>
                {t('dashboard.page.collect')}
              </Button>
            }
          />
        ) : (
          <UsageDetailsTable rows={tableRows} />
        )}

        {!usageUnavailable && (
          <UsageParserHealth
            variant="dashboard"
            refreshKey={healthRefreshKey}
            visibleAgentIds={parseVisibleIds}
          />
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
      <p className="mt-1 text-title font-semibold tracking-tight">{value}</p>
    </Card>
  );
}

function UsageOverviewSkeleton() {
  return (
    <div className={pageRhythm.blocks}>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {Array.from({ length: 5 }).map((_, i) => (
          <Skeleton key={i} className="h-20" />
        ))}
      </div>
      <Card className="p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="mt-4 h-56 w-full" />
      </Card>
      <Card className="space-y-3 p-4">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-full" />
      </Card>
    </div>
  );
}
