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
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
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
  accountsForAgent,
  liveAuthProbeForAgent,
  providersForAgent,
  useAgentStatusesOptional,
  useConnectionPool,
} from '@/app/runtime';
import {
  deleteAccount,
  importCurrentLogin,
  listAccounts,
  refreshLiveAuthState,
  refreshToken,
  switchAccount,
  undoSwitchAccount,
} from '@/lib/api/account';
import { openAgentConfigDir } from '@/lib/api/install';
import { getBackendFeatures } from '@/lib/api/backend-features';
import {
  deleteProvider,
  deleteProviders,
  importProviderLive,
  switchPreview as providerSwitchPreview,
  switchProvider,
  testLatency,
  undoSwitch as undoProviderSwitch,
} from '@/lib/api/provider';
import { isCapabilityBlocked, providerCapabilityGate } from '@/lib/capability';
import { accountActionPolicy } from '@/lib/backend/contracts/account-actions';
import { attachLiveAgentAuth } from '@/lib/backend/contracts/auth-state';
import { logger } from '@/lib/logger';
import { liveConfigPaths } from '@/lib/provider-detect';
import type { ConnectionUsageMap } from '@/lib/connect-flow/types';
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
  isCurrentSwitchPreviewRequest,
  isLiveAuthDiscoveryDeferred,
  liveApiKeyImportGate,
  liveAuthDiscoveryKind,
  liveAuthImportGate,
  mergeConnectionEntries,
  withProviderLatency,
  type ConnectionEntry,
  type ConnectionFilter,
} from './connection-model';

const log = logger.scope('connections:list');

function errorCode(error: unknown): string {
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return error.code;
  }
  if (error instanceof Error && error.name) return error.name;
  return 'unknown';
}

export function ConnectionList({
  agentId,
  agentStatuses,
  initialFilter = 'all',
  usageMap,
  onReuseRequest,
  adapterGeneratedProviderIds,
}: {
  agentId: AgentId;
  /** Shared application-level Agent detection; do not re-probe per list. */
  agentStatuses: AgentStatus[];
  /** 深链 ?mode= 映射到初始筛选 */
  initialFilter?: ConnectionFilter;
  /** 页面层计算的用途反查；未传则行上不带 usage（视觉与现状等价） */
  usageMap?: ConnectionUsageMap;
  /** 凭据侧进入 ConnectFlow；未传则不渲染「用于其他 Agent」 */
  onReuseRequest?: (entry: ConnectionEntry) => void;
  /** profiles 的 generatedProviderId 集合；命中的 Provider 不显示复用入口 */
  adapterGeneratedProviderIds?: ReadonlySet<string>;
}) {
  const { toast } = useToast();
  const backendFeatures = getBackendFeatures();
  const sharedAgentStatus = useAgentStatusesOptional();
  const pool = useConnectionPool();
  const meta = AGENT_MAP[agentId];
  const paths = liveConfigPaths(agentId);
  const accounts = React.useMemo(
    () => accountsForAgent(pool.accounts, agentId),
    [agentId, pool.accounts],
  );
  const providers = React.useMemo(
    () => providersForAgent(pool.providers, agentId),
    [agentId, pool.providers],
  );
  const poolEmpty = pool.accounts.length === 0 && pool.providers.length === 0;
  const phase: 'loading' | 'error' | 'ready' =
    pool.state === 'idle' || (pool.state === 'loading' && poolEmpty)
      ? 'loading'
      : pool.state === 'error' && poolEmpty
        ? 'error'
        : 'ready';
  const error = pool.errors.accounts ?? pool.errors.providers ?? null;
  const refreshing = pool.refreshing;
  const loadedAgentId = pool.state === 'idle' ? null : agentId;
  const accountsFailed = Boolean(pool.errors.accounts);
  const providersFailed = Boolean(pool.errors.providers);
  const poolWarning = [
    accountsFailed ? '官方登录' : null,
    providersFailed ? 'API Key' : null,
  ].filter((label): label is string => Boolean(label));

  const [filter, setFilter] = React.useState<ConnectionFilter>(initialFilter);

  const [switchEntry, setSwitchEntry] = React.useState<ConnectionEntry | null>(null);
  const [switchPreview, setSwitchPreview] = React.useState<SwitchPreview | undefined>();
  const [switching, setSwitching] = React.useState(false);
  const [previewLoading, setPreviewLoading] = React.useState(false);

  const [deleteEntry, setDeleteEntry] = React.useState<ConnectionEntry | null>(null);
  const [deleting, setDeleting] = React.useState(false);
  const [importConfirmOpen, setImportConfirmOpen] = React.useState(false);
  const [deleteAllConfirmOpen, setDeleteAllConfirmOpen] = React.useState(false);
  const [deletingAll, setDeletingAll] = React.useState(false);

  /** 账号池遗留纯 API Key（仅密钥）编辑 */
  const [editAccountKey, setEditAccountKey] = React.useState<Account | null>(null);
  /** API Key 设置（含官方/自定义端点，写入 provider 池） */
  const [apiKeyDialogOpen, setApiKeyDialogOpen] = React.useState(false);
  const [editProvider, setEditProvider] = React.useState<Provider | null>(null);
  const [importing, setImporting] = React.useState(false);
  const [importingAccount, setImportingAccount] = React.useState(false);
  const [discoveredAuth, setDiscoveredAuth] = React.useState<{
    agentId: AgentId;
    kind: 'account' | 'provider';
  } | null>(null);
  const [testingId, setTestingId] = React.useState<string | null>(null);
  const [latencyById, setLatencyById] = React.useState<Record<string, number>>({});

  const accountsRef = React.useRef(accounts);
  accountsRef.current = accounts;
  const providersRef = React.useRef(providers);
  providersRef.current = providers;
  const probeSignatureRef = React.useRef<{ agentId: AgentId; value: string } | null>(null);
  const prevAgentRef = React.useRef<AgentId | null>(null);
  const previewGeneration = React.useRef(0);

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
  const liveAuthProbe = liveAuthProbeForAgent(sharedAgentStatus, agentId) ?? null;
  const liveAuthProbeLoading =
    sharedAgentStatus.state === 'idle' ||
    sharedAgentStatus.state === 'loading' ||
    sharedAgentStatus.refreshing;
  const liveAuthImport = React.useMemo(
    () => liveAuthImportGate(liveAuthProbe, liveAuthProbeLoading, agentId),
    [agentId, liveAuthProbe, liveAuthProbeLoading],
  );
  const liveApiKeyImport = React.useMemo(
    () => liveApiKeyImportGate(liveAuthProbe, liveAuthProbeLoading, agentId),
    [agentId, liveAuthProbe, liveAuthProbeLoading],
  );
  const discoveredAuthForCurrentAgent =
    discoveredAuth?.agentId === agentId ? discoveredAuth.kind : null;

  const reload = React.useCallback(async () => {
    await pool.reload();
  }, [pool.reload]);

  React.useEffect(() => {
    if (pool.state === 'idle' || pool.state === 'loading') return;
    log.info('pool loaded', {
      agentId,
      accounts: accounts.length,
      providers: providers.length,
      currentAccount: accounts.find((account) => account.isCurrent)?.id ?? null,
      currentProvider: providers.find((provider) => provider.isCurrent)?.id ?? null,
      accountsFailed,
      providersFailed,
    });
  }, [accounts, accountsFailed, agentId, pool.state, providers, providersFailed]);

  // agent 变化：复位筛选/弹层；不 remount、不整表 skeleton。数据来自共享连接池。
  React.useEffect(() => {
    const prev = prevAgentRef.current;
    const first = prev === null;
    prevAgentRef.current = agentId;

    if (!first) {
      previewGeneration.current += 1;
      setLatencyById({});
      setFilter(initialFilter);
      setSwitchEntry(null);
      setSwitchPreview(undefined);
      setPreviewLoading(false);
      setDeleteEntry(null);
      setApiKeyDialogOpen(false);
      setEditProvider(null);
      setEditAccountKey(null);
      setDiscoveredAuth(null);
      setImportConfirmOpen(false);
      setDeleteAllConfirmOpen(false);
    }
  }, [agentId, initialFilter]);

  React.useEffect(() => {
    // An in-flight first load still has empty rows; do not treat that as a new login.
    if (pool.state === 'idle' || pool.state === 'loading') return;
    if (loadedAgentId !== agentId || !liveAuthProbe) {
      if (loadedAgentId === agentId) setDiscoveredAuth(null);
      return;
    }

    const kind = liveAuthProbe.kind?.trim().toLowerCase() ?? '';
    const isOAuth = kind === 'oauth' || kind === 'file-auth' || kind === 'file-auth.json';
    const signature = `${kind}:${liveAuthProbe.hasCredentials}:${liveAuthProbe.revision ?? liveAuthProbe.summary}`;
    const previous = probeSignatureRef.current;
    const changed = previous?.agentId !== agentId || previous.value !== signature;
    const discoveryInput = {
      poolState: pool.state,
      probe: liveAuthProbe,
      accounts: accountsRef.current,
      providers: providersRef.current,
      accountsFailed,
      providersFailed,
    };
    // A partial/error pool is not a negative result. Leave the signature
    // unstamped so a later successful refresh can still surface discovery.
    if (isLiveAuthDiscoveryDeferred(discoveryInput)) return;
    if (!changed) return;

    probeSignatureRef.current = { agentId, value: signature };
    const discovered = liveAuthDiscoveryKind(discoveryInput);
    const isSubsequentDiscovery = previous?.agentId === agentId;
    const hasExistingOAuth = accountsRef.current.some((account) => account.kind === 'oauth');
    // Grok rotates access/refresh tokens in auth.json during normal use.
    // Reconcile the current pool row automatically when a live revision changes
    // instead of presenting a duplicate-import prompt.
    if (isSubsequentDiscovery && isOAuth && hasExistingOAuth) {
      void reload();
    }
    setDiscoveredAuth(discovered ? { agentId, kind: discovered } : null);
  }, [accountsFailed, agentId, liveAuthProbe, loadedAgentId, pool.state, providersFailed, reload]);

  const liveAgentStatus = agentStatuses.find((status) => status.agentId === agentId);
  const accountsWithLiveAuth = React.useMemo(
    () => accounts.map((account) => attachLiveAgentAuth(account, liveAgentStatus)),
    [accounts, liveAgentStatus],
  );

  const entries = React.useMemo(() => {
    const merged = mergeConnectionEntries(accountsWithLiveAuth, providers, usageMap).map((e) => {
      if (e.source !== 'provider') return e;
      const ms = latencyById[e.id];
      return ms !== undefined ? withProviderLatency(e, ms) : e;
    });
    return merged;
  }, [accountsWithLiveAuth, providers, latencyById, usageMap]);

  const counts = React.useMemo(() => countByKind(entries), [entries]);
  const visible = React.useMemo(
    () => filterConnectionEntries(entries, filter),
    [entries, filter],
  );
  const currentEntry = entries.find((e) => e.isCurrent);

  const openSwitch = async (entry: ConnectionEntry) => {
    const requestedAgentId = agentId;
    const generation = ++previewGeneration.current;
    log.info('open switch preview', {
      agentId: requestedAgentId,
      source: entry.source,
      id: entry.id,
    });
    setPreviewLoading(true);
    try {
      if (entry.source === 'account' && entry.account) {
        if (accountsBlocked) {
          toast({
            title: '无法切换账号',
            description: accountsBlockReason,
            variant: 'danger',
          });
          return;
        }
        // 统一列表下「当前」可能是 API Key 配置：回存说明用列表 current
        const currentLabel = currentEntry?.title;
        if (!isCurrentSwitchPreviewRequest(
          requestedAgentId,
          agentId,
          generation,
          previewGeneration.current,
        )) return;
        setSwitchPreview({
          backfillSummary: currentLabel
            ? `当前连接「${currentLabel}」将先保存回连接池并备份`
            : '当前没有需要先保存的生效连接',
          backupPath: `~/.agenthub/backups/${requestedAgentId}/`,
          processWarning: agentStatuses.find((s) => s.agentId === requestedAgentId)?.running
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
        const preview = await providerSwitchPreview(requestedAgentId, entry.id);
        if (!isCurrentSwitchPreviewRequest(
          requestedAgentId,
          agentId,
          generation,
          previewGeneration.current,
        )) return;
        setSwitchPreview(preview);
        setSwitchEntry(entry);
      } else {
        log.warn('switch entry missing payload', {
          agentId: requestedAgentId,
          id: entry.id,
          source: entry.source,
        });
        toast({
          title: '无法切换',
          description: '连接数据不完整，请刷新后重试',
          variant: 'danger',
        });
      }
    } catch (e) {
      if (!isCurrentSwitchPreviewRequest(
        requestedAgentId,
        agentId,
        generation,
        previewGeneration.current,
      )) return;
      log.error('switch preview failed', {
        agentId: requestedAgentId,
        id: entry.id,
        source: entry.source,
        errorCode: errorCode(e),
      });
      toast({
        title: '无法预览切换',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      if (isCurrentSwitchPreviewRequest(
        requestedAgentId,
        agentId,
        generation,
        previewGeneration.current,
      )) {
        setPreviewLoading(false);
      }
    }
  };

  const confirmSwitch = async () => {
    if (!switchEntry) return;
    const target = switchEntry;
    setSwitching(true);
    try {
      if (target.source === 'account') {
        if (accountsBlocked) {
          toast({
            title: '无法切换账号',
            description: accountsBlockReason,
            variant: 'danger',
          });
          setSwitchEntry(null);
          setSwitchPreview(undefined);
          return;
        }
        await switchAccount(agentId, target.id);
        log.info('switch account ok', { agentId, id: target.id });
        toast({
          title: `已切换到 ${target.title}`,
          description: '已写入本机；其它连接已取消生效',
          variant: 'success',
          duration: 5000,
          ...(backendFeatures.accountUndoSwitch
            ? {
                actionLabel: '撤销',
                onAction: () => {
                  void undoSwitchAccount(agentId)
                    .then((ok) => {
                      if (ok) {
                        toast({ title: '已撤销切换' });
                        return reload();
                      }
                      toast({
                        title: '无法撤销',
                        description: '没有可回滚的切换记录',
                        variant: 'danger',
                      });
                    })
                    .catch((e) => {
                      toast({
                        title: '撤销失败',
                        description: e instanceof Error ? e.message : String(e),
                        variant: 'danger',
                      });
                    });
                },
              }
            : {}),
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
        toast({
          title: `已切换到 ${target.title}`,
          description: '已写入本机；其它连接已取消生效',
          variant: 'success',
          duration: 5000,
          ...(backendFeatures.providerUndoSwitch
            ? {
                actionLabel: '撤销',
                onAction: () => {
                  void undoProviderSwitch(agentId)
                    .then((ok) => {
                      if (ok) {
                        toast({ title: '已撤销切换' });
                        return reload();
                      }
                      toast({
                        title: '无法撤销',
                        description: '没有可回滚的切换记录',
                        variant: 'danger',
                      });
                    })
                    .catch((e) => {
                      toast({
                        title: '撤销失败',
                        description: e instanceof Error ? e.message : String(e),
                        variant: 'danger',
                      });
                    });
                },
              }
            : {}),
        });
      }
      setSwitchEntry(null);
      setSwitchPreview(undefined);
      // 若当前筛选会把新 current 滤掉，切回全部，否则看起来「完全没变」
      if (filter !== 'all' && filter !== target.kind) {
        setFilter('all');
      }
    } catch (e) {
      log.error('switch failed', {
        agentId,
        id: target.id,
        source: target.source,
        errorCode: errorCode(e),
      });
      toast({
        title: '切换失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
      await reload();
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

  const requestImportProvider = () => {
    if (!liveApiKeyImport.enabled) {
      toast({
        title: '无法导入当前 API Key',
        description: liveApiKeyImport.reason,
        variant: 'danger',
      });
      return;
    }
    setImportConfirmOpen(true);
  };

  const confirmImportProvider = async () => {
    setImporting(true);
    try {
      const imported = await importProviderLive(agentId);
      setDiscoveredAuth(null);
      setImportConfirmOpen(false);
      toast({
        title: `已同步「${imported.name}」`,
        description: '已保存到 AgentHub 连接池；本机配置文件未被修改。',
        variant: 'success',
      });
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
    const action = entry.account ? accountActionPolicy(entry.account) : undefined;
    if (!action) return;
    try {
      if (action.kind === 'sync-current-login') {
        // list_accounts performs the non-destructive Grok live reconciliation;
        // importCurrentLogin remains an explicit new-authorization import.
        await listAccounts(agentId);
        refreshLiveAuthState(agentId);
        toast({
          title: '已同步当前登录',
          description: '已读取 Grok CLI 当前登录凭据。',
          variant: 'success',
        });
        return;
      }
      await refreshToken(agentId, entry.id);
      toast({ title: action.label, description: entry.title, variant: 'success' });
    } catch (e) {
      toast({
        title: `${action.label}失败`,
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

  const requestDeleteAllProviders = () => {
    if (providers.length === 0) return;
    setDeleteAllConfirmOpen(true);
  };

  const confirmDeleteAllProviders = async () => {
    if (providers.length === 0) return;
    const hadCurrent = providers.some((p) => p.isCurrent);
    const count = providers.length;
    setDeletingAll(true);
    try {
      await deleteProviders(agentId, providers.map((provider) => provider.id));
      setDeleteAllConfirmOpen(false);
      toast({
        title: `已将全部 ${count} 条 API Key 配置移入回收站`,
        description: deleteConnectionToastDescription({ isCurrent: hadCurrent }),
      });
    } catch (e) {
      toast({
        title: '批量删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setDeletingAll(false);
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
          onSelect={() => requestImportProvider()}
        >
          <Import className="h-4 w-4" /> 导入当前 API Key
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => void handleOpenConfigDir()}>
          <FolderOpen className="h-4 w-4" /> 打开配置目录
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  const moreMenu =
    providers.length > 0 ? (
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" disabled={deletingAll}>
            {deletingAll ? '删除中…' : '更多'} <ChevronDown className="h-3.5 w-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-[13rem]">
          <DropdownMenuItem
            className="text-danger"
            disabled={deletingAll}
            onSelect={() => requestDeleteAllProviders()}
          >
            <Trash2 className="h-4 w-4" /> {deletingAll ? '删除中…' : '全部移入回收站'}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    ) : null;

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
        {(phase === 'ready' || phase === 'error') && (
          <div className="flex items-center gap-2">
            {moreMenu}
            {addMenu}
          </div>
        )}
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

      {discoveredAuthForCurrentAgent && (
        <div className="mb-3 flex items-center justify-between gap-3 rounded-card border border-info/40 bg-info/5 px-3 py-2 text-xs text-secondary">
          <span>
            检测到本机新的
            {discoveredAuthForCurrentAgent === 'account' ? '官方登录' : ' API Key'} 授权信息，可导入到连接列表。
          </span>
          <span className="flex shrink-0 gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() =>
                void (discoveredAuthForCurrentAgent === 'account'
                  ? handleImportAccount()
                  : requestImportProvider())
              }
              disabled={
                discoveredAuthForCurrentAgent === 'account'
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
        <ErrorState error={error} onRetry={() => void reload()} />
      )}

      {phase === 'ready' && poolWarning.length > 0 && (
        <div className="mb-3 rounded-card border border-warning/40 bg-warning/5 px-3 py-2 text-xs text-secondary" role="alert">
          {`部分连接未能加载：${poolWarning.join('、')}。已保留其余可用数据。`}
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="ml-2 h-auto px-1 py-0 text-xs"
            onClick={() => void reload()}
          >
            重试
          </Button>
        </div>
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

          {entries.length === 0 && (accountsFailed || providersFailed) && (
            <EmptyState
              icon={KeyRound}
              title={`无法完整读取 ${meta.name} 的连接`}
              description="部分连接池加载失败，请重试后再判断是否为空。"
              actionLabel="重试"
              onAction={() => void reload()}
            />
          )}

          {entries.length === 0 && !accountsFailed && !providersFailed && (
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
                  onTest={
                    backendFeatures.providerTestLatency
                      ? (e) => void handleTest(e)
                      : undefined
                  }
                  onOpenConfigDir={() => void handleOpenConfigDir()}
                  onReuseRequest={onReuseRequest}
                  adapterGeneratedProviderIds={adapterGeneratedProviderIds}
                  canEditProvider={providerGate.canManage}
                  canSwitchProvider={providerGate.canSwitch}
                  canSwitchAccount={!accountsBlocked}
                  accountSwitchBlockedReason={accountsBlockReason}
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
        onOpenChange={(open) =>
          closeConfirmationOnOpenChange(open, deleting, () => setDeleteEntry(null))
        }
      >
        <DialogContent
          className="max-w-sm"
          hideClose={deleting}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(deleting, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(deleting, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(deleting, event)}
        >
          <DialogHeader>
            <DialogTitle>删除「{deleteEntry?.title}」？</DialogTitle>
            <DialogDescription>
              {deleteEntry
                ? deleteConnectionDialogDescription(deleteEntry)
                 : '会移入回收站；不会修改本机配置文件。'}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={deleting} onClick={() => setDeleteEntry(null)}>
              取消
            </Button>
            <Button variant="danger" disabled={deleting} onClick={() => void confirmDelete()}>
              {deleting ? '删除中…' : '删除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={importConfirmOpen}
        onOpenChange={(open) =>
          closeConfirmationOnOpenChange(open, importing, () => setImportConfirmOpen(false))
        }
      >
        <DialogContent
          className="max-w-sm"
          hideClose={importing}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(importing, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(importing, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(importing, event)}
        >
          <DialogHeader>
            <DialogTitle>导入当前 API Key？</DialogTitle>
            <DialogDescription>
              将读取本机当前 API 配置；已有本机导入记录会更新，不会重复创建。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={importing}
              onClick={() => setImportConfirmOpen(false)}
            >
              取消
            </Button>
            <Button disabled={importing} onClick={() => void confirmImportProvider()}>
              {importing ? '导入中…' : '确认导入'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={deleteAllConfirmOpen}
        onOpenChange={(open) =>
          closeConfirmationOnOpenChange(open, deletingAll, () => setDeleteAllConfirmOpen(false))
        }
      >
        <DialogContent
          className="max-w-sm"
          hideClose={deletingAll}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(deletingAll, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(deletingAll, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(deletingAll, event)}
        >
          <DialogHeader>
            <DialogTitle>全部移入回收站？</DialogTitle>
            <DialogDescription>
              {`确定将 ${meta.name} 的全部 ${providers.length} 条 API Key 配置移入回收站？不会修改本机配置文件。`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={deletingAll}
              onClick={() => setDeleteAllConfirmOpen(false)}
            >
              取消
            </Button>
            <Button
              variant="danger"
              disabled={deletingAll}
              onClick={() => void confirmDeleteAllProviders()}
            >
              {deletingAll ? '删除中…' : '移入回收站'}
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
        }}
      />
      <ConnectionTrashButton agentId={agentId} />
    </div>
  );
}
