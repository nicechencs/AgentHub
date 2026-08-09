/**
 * 统一连接列表：OAuth + API Key（含原供应商/端点配置）。
 * 后端表/API 仍分离；本组件负责加载、筛选、切换、增删编辑入口。
 */
import * as React from 'react';
import {
  ChevronDown,
  DownloadCloud,
  FolderOpen,
  Import,
  KeyRound,
  Plus,
  Trash2,
} from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SwitchConfirmDialog } from '@/components/shared/SwitchConfirmDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { AGENT_MAP } from '@/config/agents';
import {
  deleteAccount,
  importCurrentLogin,
  listAccounts,
  probeLiveAuth,
  refreshToken,
  switchAccount,
  undoSwitchAccount,
  type LiveAuthProbe,
} from '@/lib/api/account';
import { openAgentConfigDir } from '@/lib/api/install';
import {
  deleteProvider,
  importProviderLive,
  listProviders,
  switchPreview as providerSwitchPreview,
  switchProvider,
  testLatency,
  undoSwitch as undoProviderSwitch,
} from '@/lib/api/provider';
import { isCapabilityBlocked, providerCapabilityGate } from '@/lib/capability';
import { logger } from '@/lib/logger';
import { liveConfigPaths } from '@/lib/provider-detect';
import type { Account, AgentId, AgentStatus, Provider, SwitchPreview } from '@/lib/types';
import { cn } from '@/lib/utils';
import { ApiKeyAccountDialog } from '@/pages/accounts/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/pages/providers/ProviderEditDialog';
import { ConnectionCard } from './ConnectionCard';
import { ConnectionTrashButton } from './ConnectionTrashButton';
import {
  CONNECTION_FILTERS,
  countByKind,
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  filterConnectionEntries,
  liveApiKeyImportGate,
  liveAuthImportGate,
  mergeConnectionEntries,
  withProviderLatency,
  type ConnectionEntry,
  type ConnectionFilter,
} from './connection-model';

const log = logger.scope('connections:list');

/** 列表切换后通知父级刷新「当前生效」摘要（勿依赖陈旧 doctor statuses） */
export type ConnectionPoolSnapshot = {
  agentId: AgentId;
  accounts: Account[];
  providers: Provider[];
  current: ConnectionEntry | undefined;
};

export function ConnectionList({
  agentId,
  agentStatuses,
  onPoolChanged,
  onSnapshot,
  initialFilter = 'all',
}: {
  agentId: AgentId;
  /** Shared application-level Agent detection; do not re-probe per list. */
  agentStatuses: AgentStatus[];
  onPoolChanged?: () => void;
  onSnapshot?: (snap: ConnectionPoolSnapshot) => void;
  /** 深链 ?mode= 映射到初始筛选 */
  initialFilter?: ConnectionFilter;
}) {
  const { toast } = useToast();
  const meta = AGENT_MAP[agentId];
  const paths = liveConfigPaths(agentId);

  const [accounts, setAccounts] = React.useState<Account[]>([]);
  const [providers, setProviders] = React.useState<Provider[]>([]);
  const [phase, setPhase] = React.useState<'loading' | 'error' | 'ready'>('loading');
  const [loadedAgentId, setLoadedAgentId] = React.useState<AgentId | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const [filter, setFilter] = React.useState<ConnectionFilter>(initialFilter);

  const [switchEntry, setSwitchEntry] = React.useState<ConnectionEntry | null>(null);
  const [switchPreview, setSwitchPreview] = React.useState<SwitchPreview | undefined>();
  const [switching, setSwitching] = React.useState(false);
  const [previewLoading, setPreviewLoading] = React.useState(false);

  const [deleteEntry, setDeleteEntry] = React.useState<ConnectionEntry | null>(null);
  const [deleting, setDeleting] = React.useState(false);

  /** 账号池遗留纯 API Key（仅密钥）编辑 */
  const [editAccountKey, setEditAccountKey] = React.useState<Account | null>(null);
  /** API Key 设置（含官方/自定义端点，写入 provider 池） */
  const [apiKeyDialogOpen, setApiKeyDialogOpen] = React.useState(false);
  const [editProvider, setEditProvider] = React.useState<Provider | null>(null);
  const [importing, setImporting] = React.useState(false);
  const [importingAccount, setImportingAccount] = React.useState(false);
  const [liveAuthProbe, setLiveAuthProbe] = React.useState<LiveAuthProbe | null>(null);
  const [liveAuthProbeLoading, setLiveAuthProbeLoading] = React.useState(true);
  const [discoveredAuth, setDiscoveredAuth] = React.useState<'account' | 'provider' | null>(null);
  const [testingId, setTestingId] = React.useState<string | null>(null);
  const [latencyById, setLatencyById] = React.useState<Record<string, number>>({});

  const onSnapshotRef = React.useRef(onSnapshot);
  onSnapshotRef.current = onSnapshot;
  const onPoolChangedRef = React.useRef(onPoolChanged);
  onPoolChangedRef.current = onPoolChanged;
  const accountsRef = React.useRef(accounts);
  accountsRef.current = accounts;
  const providersRef = React.useRef(providers);
  providersRef.current = providers;
  const probeSignatureRef = React.useRef<string | null>(null);
  const probeRunRef = React.useRef(0);

  /** 请求代数：快速连点 Agent 时丢弃过期响应 */
  const loadGenRef = React.useRef(0);
  const prevAgentRef = React.useRef<AgentId | null>(null);
  const [refreshing, setRefreshing] = React.useState(false);

  const accountsBlocked = React.useMemo(() => {
    const st = agentStatuses.find((s) => s.agentId === agentId);
    return isCapabilityBlocked(st?.capabilities?.accountSwitch);
  }, [agentStatuses, agentId]);
  const accountsBlockReason =
    agentStatuses.find((s) => s.agentId === agentId)?.capabilities?.accountSwitch?.reason ??
    '该 Agent 不支持账号池切换';

  const providerCapabilities =
    agentStatuses.find((s) => s.agentId === agentId)?.capabilities ?? meta?.capabilities;
  // Hand-authored Provider/API Key configs only need ConfigWrite. A missing
  // ProviderPresets contract affects preset/template entry-points, which this
  // page does not expose, so it must not disable custom provider editing.
  const providerGate = React.useMemo(
    () => providerCapabilityGate(providerCapabilities),
    [providerCapabilities],
  );
  const providerBlockReason = providerGate.reason ?? '当前 Agent 不支持 Provider 配置写入';
  const liveAuthImport = React.useMemo(
    () => liveAuthImportGate(liveAuthProbe, liveAuthProbeLoading),
    [liveAuthProbe, liveAuthProbeLoading],
  );
  const liveApiKeyImport = React.useMemo(
    () => liveApiKeyImportGate(liveAuthProbe, liveAuthProbeLoading),
    [liveAuthProbe, liveAuthProbeLoading],
  );

  const publish = React.useCallback(
    (accs: Account[], provs: Provider[], forAgent: AgentId) => {
      const current = mergeConnectionEntries(accs, provs).find((e) => e.isCurrent);
      onSnapshotRef.current?.({
        agentId: forAgent,
        accounts: accs,
        providers: provs,
        current,
      });
      onPoolChangedRef.current?.();
    },
    [],
  );

  /**
   * full：首屏 skeleton
   * soft：换 Agent / 切换连接后不卸列表，仅轻微 refreshing，避免闪跳
   */
  const load = React.useCallback(
    async (mode: 'full' | 'soft' = 'full') => {
      const gen = ++loadGenRef.current;
      const forAgent = agentId;
      if (mode === 'full') {
        setPhase('loading');
        setError(null);
      } else {
        setRefreshing(true);
      }
      try {
        // Do not swallow listAccounts errors — empty list + silent fail
        // made Pi accounts look "missing" when backend threw.
        const [accs, provs] = await Promise.all([
          listAccounts(forAgent),
          listProviders(forAgent).catch((e) => {
            log.warn('listProviders failed; continue with accounts only', e);
            return [] as Provider[];
          }),
        ]);
        if (gen !== loadGenRef.current) return;
        log.info('pool loaded', {
          agentId: forAgent,
          mode,
          accounts: accs.length,
          providers: provs.length,
          currentAccount: accs.find((a) => a.isCurrent)?.id ?? null,
          currentProvider: provs.find((p) => p.isCurrent)?.id ?? null,
          sampleLabels: accs.slice(0, 3).map((a) => a.label),
        });
        setAccounts(accs);
        setProviders(provs);
        setLoadedAgentId(forAgent);
        setPhase('ready');
        publish(accs, provs, forAgent);
      } catch (e) {
        if (gen !== loadGenRef.current) return;
        log.error('pool load failed', e);
        setError(e);
        setPhase('error');
      } finally {
        if (gen === loadGenRef.current) setRefreshing(false);
      }
    },
    [agentId, publish],
  );

  // agent 变化：soft 拉数 + 复位筛选/弹层；不 remount、不整表 skeleton
  React.useEffect(() => {
    const prev = prevAgentRef.current;
    const first = prev === null;
    prevAgentRef.current = agentId;

    if (!first) {
      setLatencyById({});
      setFilter(initialFilter);
      setSwitchEntry(null);
      setSwitchPreview(undefined);
      setDeleteEntry(null);
      setApiKeyDialogOpen(false);
      setEditProvider(null);
      setEditAccountKey(null);
      setError(null);
    }
    void load(first ? 'full' : 'soft');
  }, [agentId, load, initialFilter]);

  const runLiveAuthProbe = React.useCallback(async () => {
    const runId = ++probeRunRef.current;
    setLiveAuthProbeLoading(true);
    try {
      const probe = await probeLiveAuth(agentId);
      if (runId !== probeRunRef.current) return;
      setLiveAuthProbe(probe);
      const kind = probe.kind?.trim().toLowerCase() ?? '';
      const isOAuth = kind === 'oauth' || kind === 'file-auth' || kind === 'file-auth.json';
      const isApiKey = kind === 'api_key' || kind === 'api-key' || kind === 'apikey';
      const signature = `${kind}:${probe.hasCredentials}:${probe.summary}`;
      const previousSignature = probeSignatureRef.current;
      const changed = signature !== previousSignature;
      probeSignatureRef.current = signature;
      if (probe.hasCredentials && changed) {
        const isSubsequentDiscovery = previousSignature !== null;
        if (
          isOAuth &&
          (isSubsequentDiscovery ||
            !accountsRef.current.some((account) => account.kind === 'oauth'))
        ) {
          setDiscoveredAuth('account');
        } else if (
          isApiKey &&
          (isSubsequentDiscovery ||
            (accountsRef.current.every((account) => account.kind !== 'apikey') &&
              providersRef.current.length === 0))
        ) {
          setDiscoveredAuth('provider');
        }
      }
    } catch (e) {
      if (runId !== probeRunRef.current) return;
      log.warn('live auth probe failed; disable import', e);
      setLiveAuthProbe(null);
    } finally {
      if (runId === probeRunRef.current) setLiveAuthProbeLoading(false);
    }
  }, [agentId]);

  React.useEffect(() => {
    if (loadedAgentId !== agentId) return;
    probeSignatureRef.current = null;
    setDiscoveredAuth(null);
    void runLiveAuthProbe();
    const onFocus = () => void runLiveAuthProbe();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [agentId, loadedAgentId, runLiveAuthProbe]);

  const entries = React.useMemo(() => {
    const merged = mergeConnectionEntries(accounts, providers).map((e) => {
      if (e.source !== 'provider') return e;
      const ms = latencyById[e.id];
      return ms !== undefined ? withProviderLatency(e, ms) : e;
    });
    return merged;
  }, [accounts, providers, latencyById]);

  const counts = React.useMemo(() => countByKind(entries), [entries]);
  const visible = React.useMemo(
    () => filterConnectionEntries(entries, filter),
    [entries, filter],
  );
  const currentEntry = entries.find((e) => e.isCurrent);

  /** 切换成功后立刻改本地 current，再 soft 拉库校正 */
  const applyLocalCurrent = (entry: ConnectionEntry) => {
    if (entry.source === 'account') {
      setAccounts((prev) =>
        prev.map((a) => ({
          ...a,
          isCurrent: a.id === entry.id,
        })),
      );
      setProviders((prev) => prev.map((p) => ({ ...p, isCurrent: false })));
    } else {
      setProviders((prev) =>
        prev.map((p) => ({
          ...p,
          isCurrent: p.id === entry.id,
        })),
      );
      setAccounts((prev) => prev.map((a) => ({ ...a, isCurrent: false })));
    }
  };

  const openSwitch = async (entry: ConnectionEntry) => {
    log.info('open switch preview', {
      agentId,
      source: entry.source,
      id: entry.id,
      title: entry.title,
    });
    setPreviewLoading(true);
    try {
      if (entry.source === 'account' && entry.account) {
        // 统一列表下「当前」可能是 API Key 配置：回存说明用列表 current
        const currentLabel = currentEntry?.title;
        setSwitchPreview({
          backfillSummary: currentLabel
            ? `当前连接「${currentLabel}」将先保存回连接池并备份`
            : '当前没有需要先保存的生效连接',
          backupPath: `~/.agenthub/backups/${agentId}/`,
          processWarning: agentStatuses.find((s) => s.agentId === agentId)?.running
            ? `${meta.name} 正在运行，切换后需重启生效`
            : undefined,
        });
        setSwitchEntry(entry);
      } else if (entry.source === 'provider' && entry.provider) {
        if (!providerGate.canSwitch) {
          toast({
            title: '无法切换 Provider',
            description: providerBlockReason,
            variant: 'danger',
          });
          return;
        }
        const preview = await providerSwitchPreview(agentId, entry.id);
        setSwitchPreview(preview);
        setSwitchEntry(entry);
      } else {
        log.warn('switch entry missing payload', { entry });
        toast({
          title: '无法切换',
          description: '连接数据不完整，请刷新后重试',
          variant: 'danger',
        });
      }
    } catch (e) {
      log.error('switch preview failed', e);
      toast({
        title: '无法预览切换',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setPreviewLoading(false);
    }
  };

  const confirmSwitch = async () => {
    if (!switchEntry) return;
    const target = switchEntry;
    setSwitching(true);
    try {
      if (target.source === 'account') {
        await switchAccount(agentId, target.id);
        log.info('switch account ok', { agentId, id: target.id });
        applyLocalCurrent(target);
        toast({
          title: `已切换到 ${target.title}`,
          description: '已写入本机；其它连接已取消生效',
          variant: 'success',
          actionLabel: '撤销',
          onAction: () => {
            void undoSwitchAccount(agentId).then((ok) => {
              if (ok) {
                void load('soft');
                toast({ title: '已撤销切换' });
              }
            });
          },
          duration: 5000,
        });
      } else {
        if (!providerGate.canSwitch) {
          toast({
            title: '无法切换 Provider',
            description: providerBlockReason,
            variant: 'danger',
          });
          setSwitchEntry(null);
          setSwitchPreview(undefined);
          return;
        }
        await switchProvider(agentId, target.id);
        log.info('switch provider ok', { agentId, id: target.id });
        applyLocalCurrent(target);
        toast({
          title: `已切换到 ${target.title}`,
          description: '已写入本机；其它连接已取消生效',
          variant: 'success',
          actionLabel: '撤销',
          onAction: () => {
            void undoProviderSwitch(agentId).then((ok) => {
              if (ok) {
                void load('soft');
                toast({ title: '已撤销切换' });
              } else {
                toast({
                  title: '无法撤销',
                  description: '当前环境不支持撤销',
                  variant: 'danger',
                });
              }
            });
          },
          duration: 5000,
        });
      }
      setSwitchEntry(null);
      setSwitchPreview(undefined);
      // 若当前筛选会把新 current 滤掉，切回全部，否则看起来「完全没变」
      if (filter !== 'all' && filter !== target.kind) {
        setFilter('all');
      }
      // 立即推父级摘要，再 soft 校验
      if (target.source === 'account') {
        publish(
          accounts.map((a) => ({ ...a, isCurrent: a.id === target.id })),
          providers.map((p) => ({ ...p, isCurrent: false })),
          agentId,
        );
      } else {
        publish(
          accounts.map((a) => ({ ...a, isCurrent: false })),
          providers.map((p) => ({ ...p, isCurrent: p.id === target.id })),
          agentId,
        );
      }
      await load('soft');
    } catch (e) {
      log.error('switch failed', e);
      toast({
        title: '切换失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
      await load('soft');
    } finally {
      setSwitching(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteEntry) return;
    const wasCurrent = deleteEntry.isCurrent;
    setDeleting(true);
    try {
      if (deleteEntry.source === 'account') {
        await deleteAccount(agentId, deleteEntry.id);
      } else {
        await deleteProvider(agentId, deleteEntry.id);
      }
      setDeleteEntry(null);
      await load('soft');
      toast({
        title: '已移入回收站',
        description: deleteConnectionToastDescription({ isCurrent: wasCurrent }),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: '删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setDeleting(false);
    }
  };

  const handleImportAccount = async () => {
    if (!liveAuthImport.enabled) {
      toast({
        title: '无法导入当前登录态',
        description: liveAuthImport.reason,
        variant: 'danger',
      });
      return;
    }
    setImportingAccount(true);
    try {
      // Pi：会把 auth.json 各 provider 拆成多行；list 时会 heal 身份字段。
      const acc = await importCurrentLogin(agentId);
      await load('soft');
      setDiscoveredAuth(null);
      toast({
        title: '已导入当前登录态',
        description: `${acc.label} 已入库`,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: '导入失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setImportingAccount(false);
    }
  };

  const handleImportProvider = async () => {
    if (!liveApiKeyImport.enabled) {
      toast({
        title: '无法导入当前 API Key',
        description: liveApiKeyImport.reason,
        variant: 'danger',
      });
      return;
    }
    if (
      !window.confirm(
        '将读取本机当前 API 配置；已有本机导入记录会更新，不会重复创建。是否继续？',
      )
    ) {
      return;
    }
    setImporting(true);
    try {
      const imported = await importProviderLive(agentId);
      setDiscoveredAuth(null);
      toast({
        title: `已同步「${imported.name}」`,
        description: '已保存到 AgentHub 连接池；本机配置文件未被修改。',
        variant: 'success',
      });
      await load('soft');
      // Import is read-only with respect to the agent's live file. Do not
      // immediately open an editable dialog when the provider writer is
      // blocked; the imported row remains available for inspection/deletion.
      if (providerGate.canManage) setEditProvider(imported);
    } catch (e) {
      toast({
        title: '导入失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setImporting(false);
    }
  };

  const handleRefreshToken = async (entry: ConnectionEntry) => {
    try {
      await refreshToken(agentId, entry.id);
      await load('soft');
      toast({ title: 'Token 已刷新', description: entry.title, variant: 'success' });
    } catch (e) {
      toast({
        title: '刷新失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  const handleTest = async (entry: ConnectionEntry) => {
    if (entry.source !== 'provider') return;
    setTestingId(entry.id);
    try {
      const ms = await testLatency(agentId, entry.id);
      setLatencyById((prev) => ({ ...prev, [entry.id]: ms }));
    } catch (e) {
      toast({
        title: '测速失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setTestingId(null);
    }
  };

  const handleOpenConfigDir = async () => {
    try {
      const path = await openAgentConfigDir(agentId);
      toast({ title: '已打开配置目录', description: path, variant: 'success' });
    } catch (e) {
      toast({
        title: '打开失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  const handleDeleteAllProviders = async () => {
    if (providers.length === 0) return;
    const hadCurrent = providers.some((p) => p.isCurrent);
    if (
      !window.confirm(
        `确定将 ${meta.name} 的全部 ${providers.length} 条 API Key 配置移入回收站？\n不会修改本机配置文件。`,
      )
    ) {
      return;
    }
    try {
      for (const p of providers) {
        await deleteProvider(agentId, p.id);
      }
      await load('soft');
      toast({
        title: `已将全部 ${providers.length} 条 API Key 配置移入回收站`,
        description: deleteConnectionToastDescription({ isCurrent: hadCurrent }),
      });
    } catch (e) {
      toast({
        title: '批量删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
      await load('soft');
    }
  };

  const handleEdit = (entry: ConnectionEntry) => {
    if (entry.source === 'account' && entry.account) {
      // 账号池遗留 API Key：轻量编辑
      if (entry.account.kind === 'apikey') {
        setEditAccountKey(entry.account);
      }
    } else if (entry.source === 'provider' && entry.provider) {
      if (!providerGate.canManage) {
        toast({
          title: '无法编辑 Provider',
          description: providerBlockReason,
          variant: 'danger',
        });
        return;
      }
      setEditProvider(entry.provider);
      setApiKeyDialogOpen(true);
    }
  };

  const addMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button>
          <Plus className="h-4 w-4" /> 添加连接 <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[13rem]">
        {!accountsBlocked && (
          <>
            <DropdownMenuItem
              disabled={!liveAuthImport.enabled || importingAccount}
              title={!liveAuthImport.enabled ? liveAuthImport.reason : undefined}
              onSelect={() => void handleImportAccount()}
            >
              <DownloadCloud className="h-4 w-4" /> 导入当前登录态
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem
          disabled={!providerGate.canManage}
          title={!providerGate.canManage ? providerBlockReason : undefined}
          onSelect={() => {
            if (!providerGate.canManage) return;
            setEditProvider(null);
            setApiKeyDialogOpen(true);
          }}
        >
          <KeyRound className="h-4 w-4" /> 添加 API Key
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={!liveApiKeyImport.enabled || importing}
          title={!liveApiKeyImport.enabled ? liveApiKeyImport.reason : undefined}
          onSelect={() => void handleImportProvider()}
        >
          <Import className="h-4 w-4" /> 导入当前 API Key
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => void handleOpenConfigDir()}>
          <FolderOpen className="h-4 w-4" /> 打开配置目录
        </DropdownMenuItem>
        {providers.length > 0 && (
          <DropdownMenuItem
            className="text-danger"
            onSelect={() => void handleDeleteAllProviders()}
          >
            <Trash2 className="h-4 w-4" /> 移入回收站
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <div>
      <div className={cn(pageRhythm.chromeRow, 'justify-between')}>
        {/* 无连接时也展示筛选，角标为 0，避免空态丢失导航 */}
        {phase === 'ready' || phase === 'error' ? (
          <SegmentedControl
            value={filter}
            onChange={setFilter}
            aria-label="连接类型筛选"
            options={CONNECTION_FILTERS.map((f) => ({
              value: f.value,
              label: f.label,
              count: counts[f.value],
            }))}
          />
        ) : (
          <span />
        )}
        {(phase === 'ready' || phase === 'error') && addMenu}
      </div>

      {accountsBlocked && (
        <div className="mb-3 rounded-card border border-border bg-subtle px-3 py-2 text-xs text-secondary">
          <span className="font-medium text-primary">{meta.name}</span>
          {' · '}
          {accountsBlockReason}
          <span className="text-muted">
            {' · '}
            {providerGate.canManage
              ? '可用 API Key'
              : '此工具暂不支持通过 AgentHub 配置 API Key'}
          </span>
        </div>
      )}

      {discoveredAuth && (
        <div className="mb-3 flex items-center justify-between gap-3 rounded-card border border-info/40 bg-info/5 px-3 py-2 text-xs text-secondary">
          <span>
            检测到本机新的
            {discoveredAuth === 'account' ? '官方登录' : ' API Key'} 授权信息，可导入到连接列表。
          </span>
          <span className="flex shrink-0 gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() =>
                void (discoveredAuth === 'account'
                  ? handleImportAccount()
                  : handleImportProvider())
              }
              disabled={
                discoveredAuth === 'account'
                  ? !liveAuthImport.enabled || importingAccount
                  : !liveApiKeyImport.enabled || importing
              }
            >
              导入
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setDiscoveredAuth(null)}
            >
              忽略
            </Button>
          </span>
        </div>
      )}

      {!accountsBlocked && !providerGate.canManage && (
        <div className="mb-3 rounded-card border border-border bg-subtle px-3 py-2 text-xs text-secondary">
          <span className="font-medium text-primary">{meta.name}</span>
          {' · 此工具暂不支持通过 AgentHub 配置 API Key'}
          <span className="text-muted"> · 可从本机导入现有配置</span>
        </div>
      )}

      {/* 仅首屏 full loading 用 skeleton；换 Agent 保持结构，避免高度塌陷闪跳 */}
      {phase === 'loading' && <ListSkeleton rows={4} />}
      {phase === 'error' && (
        <ErrorState error={error} onRetry={() => void load('soft')} />
      )}

      {phase === 'ready' && (
        <div
          className={
            refreshing ? 'pointer-events-none opacity-60 transition-opacity' : undefined
          }
          aria-busy={refreshing || undefined}
        >
          {refreshing && (
            <p className="mb-2 text-2xs text-muted" role="status">
              正在加载 {meta.name}…
            </p>
          )}

          {entries.length === 0 && (
            <EmptyState
              icon={KeyRound}
              title={`${meta.name} 暂无连接`}
              description={
                accountsBlocked
                  ? providerGate.canManage
                    ? '可添加 API Key，或从本机导入现有配置'
                    : '当前工具不支持通过 AgentHub 配置 API Key，可从本机导入现有配置'
                  : providerGate.canManage
                    ? '可添加 API Key，或导入本机现有配置'
                    : '可从本机导入现有配置；此工具暂不支持配置 API Key'
              }
              action={addMenu}
            />
          )}

          {entries.length > 0 && visible.length === 0 && (
            <EmptyState
              icon={KeyRound}
              title="没有匹配的连接"
              description="请改用其他筛选，或添加对应类型的连接。"
              actionLabel="显示全部"
              onAction={() => setFilter('all')}
            />
          )}

          {visible.length > 0 && (
            <div className={pageRhythm.stackDense}>
              {visible.map((entry) => (
                <ConnectionCard
                  key={entry.key}
                  entry={entry}
                  brandColor={meta.color}
                  switching={switching || previewLoading || refreshing}
                  testing={testingId === entry.id}
                  onSwitch={(e) => void openSwitch(e)}
                  onDelete={setDeleteEntry}
                  onEdit={handleEdit}
                  onRefreshToken={(e) => void handleRefreshToken(e)}
                  onTest={(e) => void handleTest(e)}
                  onOpenConfigDir={() => void handleOpenConfigDir()}
                  canEditProvider={providerGate.canManage}
                  canSwitchProvider={providerGate.canSwitch}
                />
              ))}
            </div>
          )}

          <p className="mt-3 text-2xs text-muted">
            本机配置：{paths.config}
            {paths.auth ? ` · ${paths.auth}` : ''}
            {' · '}
            同时只能有一条生效
          </p>
        </div>
      )}

      <SwitchConfirmDialog
        open={!!switchEntry}
        onOpenChange={(v) => {
          if (!v) {
            setSwitchEntry(null);
            setSwitchPreview(undefined);
          }
        }}
        targetName={switchEntry?.title ?? ''}
        preview={switchPreview}
        loading={switching}
        onConfirm={() => void confirmSwitch()}
      />

      <Dialog
        open={!!deleteEntry}
        onOpenChange={(v) => !v && setDeleteEntry(null)}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>删除「{deleteEntry?.title}」？</DialogTitle>
            <DialogDescription>
              {deleteEntry
                ? deleteConnectionDialogDescription(deleteEntry)
                 : '会移入回收站；不会修改本机配置文件。'}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteEntry(null)}>
              取消
            </Button>
            <Button variant="danger" disabled={deleting} onClick={() => void confirmDelete()}>
              {deleting ? '删除中…' : '删除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ApiKeyAccountDialog
        agentId={agentId}
        mode="edit"
        account={editAccountKey}
        open={!!editAccountKey}
        onOpenChange={(v) => !v && setEditAccountKey(null)}
        onSaved={() => {
          setEditAccountKey(null);
          void load('soft');
        }}
      />
      <ProviderEditDialog
        agentId={agentId}
        mode={editProvider ? 'edit' : 'add'}
        provider={editProvider}
        open={apiKeyDialogOpen}
        onOpenChange={(v) => {
          setApiKeyDialogOpen(v);
          if (!v) setEditProvider(null);
        }}
        onSaved={() => {
          setApiKeyDialogOpen(false);
          setEditProvider(null);
          void load('soft');
        }}
      />
      <ConnectionTrashButton agentId={agentId} onChanged={() => void load('soft')} />
    </div>
  );
}
