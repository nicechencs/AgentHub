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
import { AGENT_IDS, agentDisplayName } from '@/config/agents';
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
  accountsForAgent,
  getConnectionPoolSnapshot,
  providersForAgent,
  useConnectionPool,
} from '@/app/runtime';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { parseConnectionFocusFilter } from '@/lib/connection-kind';
import type { AgentId } from '@/lib/types';
import { ApiKeyAccountDialog } from '@/pages/accounts/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/pages/providers/ProviderEditDialog';
import { ConnectionTrashButton } from './ConnectionTrashButton';
import { TicketWalletList } from './TicketWalletList';
import type { TicketWalletFilter } from './ticket-wallet-model';
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
import { deleteAccount, importCurrentLogin } from '@/lib/api/account';
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
  const { installedIds, loading, state, error, reload } = useInstalledAgents();
  const pool = useConnectionPool();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();

  const allowedAgents = installedIds.length ? installedIds : AGENT_IDS;
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
  const [importingAccount, setImportingAccount] = useState(false);
  const [detailTicket, setDetailTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const guideOpenedApiKeyRef = useRef(false);

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  useEffect(() => {
    if (highlightAgentId) setAddAgentId(highlightAgentId);
  }, [highlightAgentId]);

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
    const allowed = installedIds.length ? installedIds : AGENT_IDS;
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
  }, [installedIds, searchParams, setSearchParams]);

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

  const handleDetailTicket = useCallback(
    (ticket: TicketView) => {
      setDetailTicket(ticket);
      if (ticket.sourceKind === 'provider') {
        const provider = providersForAgent(pool.providers, ticket.agentId).find(
          (p) => p.id === ticket.sourceId,
        ) ?? pool.providers.find((p) => p.id === ticket.sourceId);
        if (provider) {
          setEditProvider(provider);
          setApiKeyDialogOpen(true);
          return;
        }
      }
      if (ticket.sourceKind === 'account') {
        const account = accountsForAgent(pool.accounts, ticket.agentId).find(
          (a) => a.id === ticket.sourceId,
        ) ?? pool.accounts.find((a) => a.id === ticket.sourceId);
        if (account?.kind === 'apikey') {
          setEditAccountKey(account);
          return;
        }
      }
      toast({
        title: ticket.label,
        description: `${ticketCredentialHint(ticket)} · 所属 ${agentDisplayName(ticket.agentId)}`,
      });
    },
    [pool.accounts, pool.providers, toast],
  );

  const confirmImportLogin = async () => {
    setImportingAccount(true);
    try {
      const acc = await importCurrentLogin(addAgentId);
      setLoginImportOpen(false);
      toast({
        title: '已导入当前登录态',
        description: `${acc.label} 已入库`,
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

  const confirmDeleteDetail = async () => {
    if (!detailTicket) return;
    setDeleteBusy(true);
    try {
      if (detailTicket.sourceKind === 'account') {
        await deleteAccount(detailTicket.agentId, detailTicket.sourceId);
      } else {
        await deleteProvider(detailTicket.agentId, detailTicket.sourceId);
      }
      setDetailTicket(null);
      toast({ title: '已移入回收站', variant: 'success' });
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
          wallet
            ? `钱包 · ${wallet.tickets.length} 张票`
            : '钱包 · 官方登录 / API Key'
        }
        descriptionTip="跨 Agent 的票列表。每张真票都可「接到…」其他 Agent；生成投影不出现在本页。"
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
          title="无法读取票钱包"
          onRetry={() => void loadWallet()}
        />
      ) : (
        <TicketWalletList
          wallet={wallet}
          loading={walletLoading}
          highlightAgentId={highlightAgentId}
          initialFilter={initialFilter}
          onConnectTicket={handleConnectTicket}
          onDetailTicket={handleDetailTicket}
          addAgentId={addAgentId}
          installedAgentIds={allowedAgents}
          onPickAddAgent={setAddAgentId}
          onAddKey={() => {
            setEditProvider(null);
            setApiKeyDialogOpen(true);
          }}
          onImportLogin={() => setLoginImportOpen(true)}
        />
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
        open={Boolean(detailTicket) && !apiKeyDialogOpen && !editAccountKey}
        onOpenChange={(open) => {
          if (!open && !deleteBusy) setDetailTicket(null);
        }}
      >
        <DialogContent className="max-w-sm" hideClose={deleteBusy}>
          <DialogHeader>
            <DialogTitle>{detailTicket?.label}</DialogTitle>
            <DialogDescription>
              {detailTicket
                ? `${ticketCredentialHint(detailTicket)} · 所属 ${agentDisplayName(detailTicket.agentId)}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={deleteBusy} onClick={() => setDetailTicket(null)}>
              关闭
            </Button>
            <Button
              variant="danger"
              disabled={deleteBusy}
              onClick={() => void confirmDeleteDetail()}
            >
              {deleteBusy ? '删除中…' : '移入回收站'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ApiKeyAccountDialog
        agentId={addAgentId}
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

function ticketCredentialHint(ticket: TicketView): string {
  if (ticket.credentialClass === 'oauth') return '官方登录';
  if (ticket.credentialClass === 'api_key') return 'API Key';
  return '未识别';
}
