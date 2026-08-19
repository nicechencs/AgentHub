// Connections：全局票钱包（docs/connection-binding-model.md §5.2）
// Agent 只作筛选/高亮，不作第一导航；?agent= 高亮 active 绑定行。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { listTicketWallet, type TicketView, type TicketWallet } from '@/lib/api/tickets';
import { ConnectFlowDialog } from '@/components/connect/ConnectFlowDialog';
import {
  buildResumeConnectUrl,
  consumeConnectIntent,
  parseResumeAgentId,
  readConnectGuide,
  type ConnectGuide,
} from '@/lib/connect-flow/connect-intent';
import { createDefaultConnectFlowDeps } from '@/lib/connect-flow/default-deps';
import type { ConnectFlowEntry } from '@/lib/connect-flow/types';
import {
  getConnectionPoolSnapshot,
  useConnectionPool,
} from '@/app/runtime';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { parseConnectionFocusFilter } from '@/lib/connection-kind';
import type { AgentId } from '@/lib/types';
import { ApiKeyAccountDialog } from '@/pages/accounts/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/pages/providers/ProviderEditDialog';
import { ConnectionTrashButton } from './ConnectionTrashButton';
import { TicketWalletList } from './TicketWalletList';
import {
  extrasFromPoolSource,
  findTicketPoolSource,
  ticketAddDialogState,
  type TicketAddKind,
  type TicketWalletFilter,
} from './ticket-wallet-model';
import {
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  liveAuthCoexistenceNotice,
} from './connection-model';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { deleteAccount, importCurrentLogin, probeLiveAuth, type LiveAuthProbe } from '@/lib/api/account';
import { deleteProvider } from '@/lib/api/provider';
import type { Account, Provider } from '@/lib/types';

function parseAgentParam(raw: string | null, allowed: AgentId[]): AgentId | null {
  if (raw && allowed.includes(raw as AgentId)) return raw as AgentId;
  return null;
}

function parseWalletFilter(raw: string | null): TicketWalletFilter {
  const focus = parseConnectionFocusFilter(raw);
  if (focus === 'oauth') return 'oauth';
  if (focus === 'apikey') return 'api_key';
  if (raw?.trim().toLowerCase() === 'unknown') return 'unknown';
  return 'all';
}

export default function ConnectionsPage() {
  const { installedIds, visibleIds, hiddenIds, loading, state, error, reload } = useInstalledAgents();
  const pool = useConnectionPool();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();

  const allowedAgents = installedIds.length ? installedIds : visibleIds;
  const hiddenSet = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const highlightAgentId = parseAgentParam(searchParams.get('agent'), allowedAgents);
  const initialFilter = parseWalletFilter(searchParams.get('mode'));
  const resumeAgentId = parseResumeAgentId(searchParams.get('resume'), allowedAgents);

  const [pendingGuide, setPendingGuide] = useState<ConnectGuide | null>(null);
  const consumedGuideKeyRef = useRef<string | null>(null);

  const [wallet, setWallet] = useState<TicketWallet | null>(null);
  const [walletError, setWalletError] = useState<unknown>(null);
  const [walletLoading, setWalletLoading] = useState(true);
  const [connectEntry, setConnectEntry] = useState<ConnectFlowEntry | null>(null);
  const connectDeps = useMemo(() => createDefaultConnectFlowDeps(), []);

  /** Agent context for add/import dialogs (deep-link or picker). */
  const [addAgentId, setAddAgentId] = useState<AgentId>(
    () => highlightAgentId ?? allowedAgents[0] ?? 'claude',
  );
  const [apiKeyDialogOpen, setApiKeyDialogOpen] = useState(false);
  const [editProvider, setEditProvider] = useState<Provider | null>(null);
  const [editAccountKey, setEditAccountKey] = useState<Account | null>(null);
  const [loginImportOpen, setLoginImportOpen] = useState(false);
  const [importLiveProbe, setImportLiveProbe] = useState<LiveAuthProbe | null>(null);
  const [importProbeLoading, setImportProbeLoading] = useState(false);
  const importProbeGen = useRef(0);
  const [importingAccount, setImportingAccount] = useState(false);
  const [deleteTicket, setDeleteTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const guideOpenedApiKeyRef = useRef(false);

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  useEffect(() => {
    if (highlightAgentId) setAddAgentId(highlightAgentId);
  }, [highlightAgentId]);

  useEffect(() => {
    if (!loginImportOpen) {
      importProbeGen.current += 1;
      setImportLiveProbe(null);
      setImportProbeLoading(false);
      return;
    }
    const generation = ++importProbeGen.current;
    const agentId = addAgentId;
    setImportLiveProbe(null);
    setImportProbeLoading(true);
    void probeLiveAuth(agentId, { force: true }).then(
      (probe) => {
        if (importProbeGen.current !== generation) return;
        setImportLiveProbe(probe);
        setImportProbeLoading(false);
      },
      () => {
        if (importProbeGen.current !== generation) return;
        setImportLiveProbe(null);
        setImportProbeLoading(false);
      },
    );
  }, [addAgentId, loginImportOpen]);

  const walletGeneration = useRef(0);
  const loadWallet = useCallback(async (): Promise<boolean> => {
    const generation = ++walletGeneration.current;
    setWalletLoading(true);
    try {
      const next = await listTicketWallet();
      if (walletGeneration.current === generation) {
        setWallet(next);
        setWalletError(null);
      }
      return true;
    } catch (e) {
      if (walletGeneration.current === generation) {
        setWalletError(e);
        setWallet((prev) => prev ?? null);
      }
      return false;
    } finally {
      if (walletGeneration.current === generation) setWalletLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadWallet();
  }, [loadWallet]);

  const visibleWallet = useMemo(() => {
    if (!wallet) return null;
    if (hiddenSet.size === 0) return wallet;
    return {
      ...wallet,
      tickets: wallet.tickets.filter((ticket) => !hiddenSet.has(ticket.agentId)),
      bindings: wallet.bindings.filter((binding) => !hiddenSet.has(binding.agentId)),
    };
  }, [hiddenSet, wallet]);

  const poolReload = pool.reload;

  const handleConnectionChanged = useCallback(async () => {
    const walletOk = await loadWallet();
    const [, statusesOk] = await Promise.all([
      poolReload().catch(() => {}),
      Promise.resolve(reload()).then(
        () => true,
        () => false,
      ),
    ]);
    const poolSnapshot = getConnectionPoolSnapshot();
    const poolOk =
      poolSnapshot.state === 'ready'
      && !poolSnapshot.errors.accounts
      && !poolSnapshot.errors.providers;
    if (!walletOk || !poolOk || !statusesOk) {
      throw new Error('列表刷新失败，可手动刷新查看最新状态');
    }
  }, [loadWallet, poolReload, reload]);

  useEffect(() => {
    const allowed = installedIds.length ? installedIds : visibleIds;
    const guide = readConnectGuide(searchParams, allowed);
    if (!guide) {
      consumedGuideKeyRef.current = null;
      return;
    }
    const key = searchParams.toString();
    if (consumedGuideKeyRef.current === key) return;
    consumedGuideKeyRef.current = key;
    setPendingGuide(guide);
    if (guide.resumeAgentId) setAddAgentId(guide.resumeAgentId);
    // Prefer agent from URL when present
    const agentFromUrl = parseAgentParam(searchParams.get('agent'), allowed);
    if (agentFromUrl) setAddAgentId(agentFromUrl);
    setSearchParams(consumeConnectIntent(searchParams), { replace: true });
  }, [installedIds, visibleIds, searchParams, setSearchParams]);

  useEffect(() => {
    const intent = pendingGuide?.intent ?? null;
    if (!intent) return;
    if (intent === 'add-key') {
      guideOpenedApiKeyRef.current = true;
      setEditProvider(null);
      setApiKeyDialogOpen(true);
      setPendingGuide(null);
      return;
    }
    if (intent === 'import-login') {
      setLoginImportOpen(true);
      setPendingGuide(null);
    }
  }, [pendingGuide]);

  const handleGuideSucceeded = useCallback(() => {
    const resume = pendingGuide?.resumeAgentId ?? resumeAgentId;
    setPendingGuide(null);
    if (resume) navigate(buildResumeConnectUrl(resume));
  }, [navigate, pendingGuide, resumeAgentId]);

  const handleConnectTicket = useCallback((ticket: TicketView) => {
    setConnectEntry({
      mode: 'for-source',
      source: { kind: ticket.sourceKind, id: ticket.sourceId },
    });
  }, []);

  const extrasForTicket = useCallback(
    (ticket: TicketView) =>
      extrasFromPoolSource(ticket, findTicketPoolSource(ticket, pool.accounts, pool.providers)),
    [pool.accounts, pool.providers],
  );

  const openTicketAdd = useCallback((kind: TicketAddKind, agentId: AgentId) => {
    const next = ticketAddDialogState(kind, agentId);
    setAddAgentId(next.addAgentId);
    if (next.clearEditProvider) setEditProvider(null);
    if (next.loginImportOpen) setLoginImportOpen(true);
    if (next.apiKeyDialogOpen) setApiKeyDialogOpen(true);
  }, []);

  const handleEditTicket = useCallback(
    (ticket: TicketView) => {
      const source = findTicketPoolSource(ticket, pool.accounts, pool.providers);
      if (source.provider) {
        setEditProvider(source.provider);
        setApiKeyDialogOpen(true);
        return;
      }
      if (source.account?.kind === 'apikey') {
        setEditAccountKey(source.account);
      }
    },
    [pool.accounts, pool.providers],
  );

  const importCoexistenceNotice = liveAuthCoexistenceNotice(importLiveProbe, addAgentId);

  const confirmImportLogin = async () => {
    const coexistenceNotice = importCoexistenceNotice;
    setImportingAccount(true);
    try {
      const acc = await importCurrentLogin(addAgentId);
      setLoginImportOpen(false);
      toast({
        title: '已导入当前登录态',
        description: coexistenceNotice
          ? `${acc.label} 已入库。另一份本机凭据未导入，仍留在本机。`
          : `${acc.label} 已入库`,
        variant: 'success',
      });
      await loadWallet();
      await poolReload().catch(() => {});
      handleGuideSucceeded();
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

  const confirmDeleteTicket = async () => {
    if (!deleteTicket) return;
    const extras = extrasForTicket(deleteTicket);
    setDeleteBusy(true);
    try {
      if (deleteTicket.sourceKind === 'account') {
        await deleteAccount(deleteTicket.agentId, deleteTicket.sourceId);
      } else {
        await deleteProvider(deleteTicket.agentId, deleteTicket.sourceId);
      }
      setDeleteTicket(null);
      toast({
        title: '已移入回收站',
        description: deleteConnectionToastDescription({ isCurrent: extras.isCurrent === true }),
        variant: 'success',
      });
      await loadWallet();
      await poolReload().catch(() => {});
    } catch (e) {
      toast({
        title: '删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setDeleteBusy(false);
    }
  };

  if (loading) {
    return (
      <div>
        <PageHeader title="连接" description="钱包" descriptionTip="正在检测已安装的 Agent。" />
        <div className={pageRhythm.chrome}>
          <ListSkeleton rows={4} />
        </div>
      </div>
    );
  }

  if (state === 'error') {
    return (
      <div>
        <PageHeader title="连接" description="钱包" descriptionTip="Agent 检测失败，请重试后再管理连接。" />
        <ErrorState error={error} title="无法读取 Agent 安装状态" onRetry={() => void reload()} />
      </div>
    );
  }

  if (!loading && installedIds.length === 0) {
    return (
      <div>
        <PageHeader title="连接" description="钱包" descriptionTip="先安装 Agent，再管理连接。" />
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
          visibleWallet
            ? `钱包 · ${visibleWallet.tickets.length} 份登录`
            : '钱包 · 官方登录 / API Key'
        }
        descriptionTip="跨 Agent 的登录列表。每份登录都可「接到…」其他 Agent；生成投影不出现在本页。"
      />

      {resumeAgentId ? (
        <div className={pageRhythm.lead}>
          <Notice
            tone="info"
            actionLabel="返回继续连接"
            onAction={() => navigate(buildResumeConnectUrl(resumeAgentId))}
          >
            取消后可返回继续连接。
          </Notice>
        </div>
      ) : null}

      {walletError && !wallet ? (
        <ErrorState
          error={walletError}
          title="无法读取钱包"
          onRetry={() => void loadWallet()}
        />
      ) : (
        <>
          {walletError && wallet ? (
            <Notice
              className="mb-3 text-sm"
              tone="warning"
              actionLabel="重试"
              onAction={() => void loadWallet()}
            >
              钱包刷新失败，下方仍是上次成功加载的数据。
            </Notice>
          ) : null}
          <TicketWalletList
            wallet={visibleWallet}
            loading={walletLoading}
            highlightAgentId={highlightAgentId}
            initialFilter={initialFilter}
            onConnectTicket={handleConnectTicket}
            extrasForTicket={extrasForTicket}
            onEditTicket={handleEditTicket}
            onDeleteTicket={setDeleteTicket}
            installedAgentIds={allowedAgents}
            onAddKey={(id) => openTicketAdd('api-key', id)}
            onImportLogin={(id) => openTicketAdd('import-login', id)}
          />
        </>
      )}

      <div className="mt-4">
        <ConnectionTrashButton onChanged={() => void loadWallet()} />
      </div>

      <ConnectFlowDialog
        entry={connectEntry}
        deps={connectDeps}
        onClose={() => setConnectEntry(null)}
        onConnectionChanged={handleConnectionChanged}
        onNavigate={(to) => navigate(to)}
      />

      <Dialog
        open={loginImportOpen}
        onOpenChange={(open) =>
          closeConfirmationOnOpenChange(open, importingAccount, () => setLoginImportOpen(false))
        }
      >
        <DialogContent
          className="max-w-sm"
          hideClose={importingAccount}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
        >
          <DialogHeader>
            <DialogTitle>导入当前登录态？</DialogTitle>
            <DialogDescription>
              将读取 {agentDisplayName(addAgentId)} 本机官方 CLI 已完成的登录；AgentHub 不会在此发起新的授权。
            </DialogDescription>
          </DialogHeader>
          {importProbeLoading ? (
            <p className="text-xs text-muted">正在检测本机凭据…</p>
          ) : null}
          {importCoexistenceNotice ? (
            <Notice tone="warning">{importCoexistenceNotice}</Notice>
          ) : null}
          <DialogFooter>
            <Button
              variant="outline"
              disabled={importingAccount}
              onClick={() => setLoginImportOpen(false)}
            >
              取消
            </Button>
            <Button disabled={importingAccount} onClick={() => void confirmImportLogin()}>
              {importingAccount ? '导入中…' : '确认导入'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(deleteTicket)}
        onOpenChange={(open) => {
          if (!open && !deleteBusy) setDeleteTicket(null);
        }}
      >
        <DialogContent
          className="max-w-sm"
          hideClose={deleteBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(deleteBusy, event)}
        >
          <DialogHeader>
            <DialogTitle>移入回收站？</DialogTitle>
            <DialogDescription>
              {deleteTicket
                ? `${deleteTicket.label} · ${deleteConnectionDialogDescription({
                    isCurrent: extrasForTicket(deleteTicket).isCurrent === true,
                  })}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={deleteBusy} onClick={() => setDeleteTicket(null)}>
              取消
            </Button>
            <Button
              variant="danger"
              disabled={deleteBusy}
              onClick={() => void confirmDeleteTicket()}
            >
              {deleteBusy ? '删除中…' : '移入回收站'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ApiKeyAccountDialog
        agentId={editAccountKey?.agentId ?? addAgentId}
        mode="edit"
        account={editAccountKey}
        open={!!editAccountKey}
        onOpenChange={(v) => !v && setEditAccountKey(null)}
        onSaved={() => {
          setEditAccountKey(null);
          void loadWallet();
          void poolReload();
        }}
      />
      <ProviderEditDialog
        agentId={editProvider?.agentId ?? addAgentId}
        mode={editProvider ? 'edit' : 'add'}
        provider={editProvider}
        open={apiKeyDialogOpen}
        onOpenChange={(v) => {
          setApiKeyDialogOpen(v);
          if (!v) {
            setEditProvider(null);
            guideOpenedApiKeyRef.current = false;
          }
        }}
        onSaved={() => {
          const fromGuide = guideOpenedApiKeyRef.current;
          setApiKeyDialogOpen(false);
          setEditProvider(null);
          guideOpenedApiKeyRef.current = false;
          void loadWallet();
          void poolReload();
          if (fromGuide) handleGuideSucceeded();
        }}
      />
    </div>
  );
}
