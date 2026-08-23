// Connections：全局票钱包（docs/connection-binding-model.md §5.2）
// AgentTabStrip 筛选；?agent= 高亮并把 Tab 落到该 Agent。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import {
  bindTicket,
  isActiveBindingForAgent,
  listTicketWallet,
  type TicketView,
  type TicketWallet,
} from '@/lib/api/tickets';
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
import {
  oauthListAction,
  oauthListActionProbesQuota,
} from '@/lib/backend/contracts/account-actions';
import type { AgentId } from '@/lib/types';
import { ApiKeyAccountDialog } from '@/pages/accounts/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/pages/providers/ProviderEditDialog';
import { ConnectionTrashButton } from './ConnectionTrashButton';
import { TicketAddMenu, TicketWalletList } from './TicketWalletList';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  extrasFromPoolSource,
  filterTicketsByAgentUsage,
  findTicketPoolSource,
  scheduleAfterMenuClose,
  shouldIgnoreMenuDialogDismiss,
  ticketAddDialogState,
  type TicketAddKind,
} from './ticket-wallet-model';
import {
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  liveAuthCoexistenceNotice,
  liveAuthImportGate,
  liveApiKeyImportGate,
  liveAuthDiscoveryKind,
  liveImportDialogMode,
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
import {
  deleteAccount,
  importCurrentLogin,
  probeLiveAuth,
  refreshQuota,
  refreshToken,
  switchAccount,
  type LiveAuthProbe,
} from '@/lib/api/account';
import { deleteProvider, switchPreview, switchProvider } from '@/lib/api/provider';
import type { Account, Provider } from '@/lib/types';

function parseAgentParam(raw: string | null, allowed: AgentId[]): AgentId | null {
  if (raw && allowed.includes(raw as AgentId)) return raw as AgentId;
  return null;
}

export default function ConnectionsPage() {
  const { t } = useI18n();
  const { installedIds, visibleIds, hiddenIds, loading, state, error, reload } = useInstalledAgents();
  const pool = useConnectionPool();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();

  const allowedAgents = installedIds.length ? installedIds : visibleIds;
  const hiddenSet = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const highlightAgentId = parseAgentParam(searchParams.get('agent'), allowedAgents);
  const resumeAgentId = parseResumeAgentId(searchParams.get('resume'), allowedAgents);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>(highlightAgentId ?? 'all');
  const [refreshingTicketId, setRefreshingTicketId] = useState<string | null>(null);
  const refreshGen = useRef(0);
  const refreshInFlightRef = useRef(false);
  const [switchingTicketId, setSwitchingTicketId] = useState<string | null>(null);
  const switchGen = useRef(0);

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
  const [discoveryProbe, setDiscoveryProbe] = useState<LiveAuthProbe | null>(null);
  const [discoveryLoading, setDiscoveryLoading] = useState(false);
  const [discoveryDismissed, setDiscoveryDismissed] = useState(false);
  const discoveryProbeGen = useRef(0);
  const [deleteTicket, setDeleteTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const guideOpenedApiKeyRef = useRef(false);
  const ignoreMenuDialogDismissRef = useRef(false);

  useEffect(() => {
    if (pool.state === 'idle') void pool.ensureLoaded();
  }, [pool.ensureLoaded, pool.state]);

  useEffect(() => {
    if (highlightAgentId) {
      setAddAgentId(highlightAgentId);
      setFilterAgent(highlightAgentId);
    }
  }, [highlightAgentId]);

  useEffect(() => {
    if (filterAgent !== 'all' && hiddenSet.has(filterAgent)) {
      setFilterAgent('all');
    }
  }, [filterAgent, hiddenSet]);

  const discoveryAgentId: AgentId = filterAgent === 'all' ? addAgentId : filterAgent;

  useEffect(() => {
    if (!loginImportOpen) {
      importProbeGen.current += 1;
      setImportLiveProbe(null);
      setImportProbeLoading(false);
      return;
    }
    const generation = ++importProbeGen.current;
    const agentId = addAgentId;
    const seed = discoveryProbe?.agentId === agentId ? discoveryProbe : null;
    setImportLiveProbe(seed);
    setImportProbeLoading(!seed);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps -- seed at open; listing discoveryProbe would re-force
  }, [addAgentId, loginImportOpen]);

  useEffect(() => {
    setDiscoveryDismissed(false);
  }, [discoveryAgentId]);

  useEffect(() => {
    if (pool.state !== 'ready' && pool.state !== 'partial') return;
    const generation = ++discoveryProbeGen.current;
    setDiscoveryLoading(true);
    void probeLiveAuth(discoveryAgentId).then(
      (probe) => {
        if (discoveryProbeGen.current !== generation) return;
        setDiscoveryProbe(probe);
        setDiscoveryLoading(false);
      },
      () => {
        if (discoveryProbeGen.current !== generation) return;
        setDiscoveryProbe(null);
        setDiscoveryLoading(false);
      },
    );
  }, [discoveryAgentId, pool.state]);

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
    const tickets = wallet.tickets.filter((ticket) => !hiddenSet.has(ticket.agentId));
    const bindings = wallet.bindings.filter((binding) => !hiddenSet.has(binding.agentId));
    const ticketIds = new Set(tickets.map((ticket) => ticket.id));
    const surfaceGroups = (wallet.surfaceGroups ?? [])
      .map((group) => ({
        ...group,
        members: group.members.filter((member) => ticketIds.has(member.ticketId)),
      }))
      .filter((group) => group.members.length > 0);
    return { ...wallet, tickets, bindings, surfaceGroups };
  }, [hiddenSet, wallet]);

  const tabAgents = useMemo(
    () => visibleIds.map((id) => resolveAgentMeta(id)),
    [visibleIds],
  );

  const agentCounts = useMemo(() => {
    const tickets = visibleWallet?.tickets ?? [];
    const counts: Partial<Record<AgentTabId, number>> = { all: tickets.length };
    if (!visibleWallet) {
      for (const id of visibleIds) counts[id] = 0;
      return counts;
    }
    for (const id of visibleIds) {
      counts[id] = filterTicketsByAgentUsage(visibleWallet, tickets, id).length;
    }
    return counts;
  }, [visibleIds, visibleWallet]);

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
      throw new Error(t('connections.page.refreshFailed'));
    }
  }, [loadWallet, poolReload, reload, t]);

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

  const handleShareTicket = useCallback((ticket: TicketView) => {
    setConnectEntry({
      mode: 'for-source',
      source: { kind: ticket.sourceKind, id: ticket.sourceId },
      purpose: 'share',
    });
  }, []);

  const handleRouteTicket = useCallback((ticket: TicketView) => {
    setConnectEntry({
      mode: 'for-source',
      source: { kind: ticket.sourceKind, id: ticket.sourceId },
      purpose: 'route',
    });
  }, []);

  const handleSwitchTicket = useCallback(async (ticket: TicketView) => {
    const targetAgent = filterAgent === 'all' ? ticket.agentId : filterAgent;
    const tabCurrentId = wallet
      ? activeBindingForAgent(wallet, targetAgent)?.ticket.id ?? null
      : null;
    if (tabCurrentId === ticket.id) return;
    const generation = ++switchGen.current;
    setSwitchingTicketId(ticket.id);
    try {
      if (ticket.agentId === targetAgent) {
        if (ticket.sourceKind === 'account') {
          await switchAccount(ticket.agentId, ticket.sourceId);
        } else {
          await switchPreview(ticket.agentId, ticket.sourceId);
          await switchProvider(ticket.agentId, ticket.sourceId);
        }
      } else {
        const { binding } = await bindTicket(ticket.id, targetAgent);
        if (!isActiveBindingForAgent(binding, targetAgent)) {
          throw new Error(t('connections.list.switchFail'));
        }
      }
      if (switchGen.current !== generation) return;
      toast({ title: t('connections.list.switchOk'), variant: 'success' });
      await poolReload().catch(() => {});
      await loadWallet();
    } catch (e) {
      if (switchGen.current !== generation) return;
      toast({
        title: t('connections.list.switchFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      if (switchGen.current === generation) setSwitchingTicketId(null);
    }
  }, [filterAgent, loadWallet, poolReload, t, toast, wallet]);

  const handleRefreshTicket = useCallback(async (ticket: TicketView) => {
    if (refreshInFlightRef.current) return;
    if (ticket.sourceKind !== 'account') return;
    const source = findTicketPoolSource(ticket, pool.accounts, pool.providers);
    const account = source.account;
    if (!account) return;
    const action = oauthListAction(account);
    if (!action) return;
    refreshInFlightRef.current = true;
    const generation = ++refreshGen.current;
    setRefreshingTicketId(ticket.id);
    try {
      if (action.kind === 'sync-current-login') {
        let probe: LiveAuthProbe;
        try {
          probe = await probeLiveAuth(account.agentId, { force: true });
        } catch {
          if (refreshGen.current !== generation) return;
          toast({
            title: t('connections.import.toastFail'),
            description: t('connections.list.cannotConfirmLogin'),
            variant: 'danger',
          });
          return;
        }
        const gate = liveAuthImportGate(probe, false, account.agentId, t);
        if (!gate.enabled) {
          if (refreshGen.current !== generation) return;
          toast({
            title: t('connections.import.toastFail'),
            description: gate.reason,
            variant: 'danger',
          });
          return;
        }
        const acc = await importCurrentLogin(account.agentId);
        // Sync only refreshes auth.json; quota still needs an upstream probe
        // (same as Hub-owned refresh-credentials). Swallow probe errors so a
        // usage miss does not look like a failed login import.
        if (oauthListActionProbesQuota(action.kind)) {
          await refreshQuota(account.agentId, acc.id).catch(() => undefined);
        }
        if (refreshGen.current !== generation) return;
        const coexistenceNotice = liveAuthCoexistenceNotice(probe, account.agentId, t);
        toast({
          title: t('connections.import.toastOk'),
          description: coexistenceNotice
            ? t('connections.import.toastOkCoexist', { label: acc.label })
            : t('connections.import.toastOkDesc', { label: acc.label }),
          variant: 'success',
        });
      } else if (action.kind === 'refresh-credentials') {
        await refreshToken(account.agentId, account.id);
        await refreshQuota(account.agentId, account.id).catch(() => undefined);
        if (refreshGen.current !== generation) return;
        toast({ title: t('connections.list.refreshOk'), variant: 'success' });
      } else {
        await refreshQuota(account.agentId, account.id);
        if (refreshGen.current !== generation) return;
        toast({ title: t('connections.list.refreshOk'), variant: 'success' });
      }
      await poolReload().catch(() => {});
      await loadWallet();
    } catch (e) {
      if (refreshGen.current !== generation) return;
      toast({
        title: action.kind === 'sync-current-login'
          ? t('connections.import.toastFail')
          : t('connections.list.refreshFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      refreshInFlightRef.current = false;
      if (refreshGen.current === generation) setRefreshingTicketId(null);
    }
  }, [loadWallet, pool.accounts, pool.providers, poolReload, t, toast]);

  const extrasForTicket = useCallback(
    (ticket: TicketView) => {
      try {
        const tabCurrentTicketId = filterAgent === 'all' || !wallet
          ? undefined
          : activeBindingForAgent(wallet, filterAgent)?.ticket.id ?? null;
        return extrasFromPoolSource(
          ticket,
          findTicketPoolSource(ticket, pool.accounts, pool.providers),
          t,
          tabCurrentTicketId,
        );
      } catch {
        return null;
      }
    },
    [filterAgent, pool.accounts, pool.providers, t, wallet],
  );

  const openTicketAdd = useCallback((kind: TicketAddKind, agentId: AgentId) => {
    const next = ticketAddDialogState(kind, agentId);
    setAddAgentId(next.addAgentId);
    if (next.clearEditProvider) setEditProvider(null);
    ignoreMenuDialogDismissRef.current = true;
    if (next.loginImportOpen) setLoginImportOpen(true);
    if (next.apiKeyDialogOpen) setApiKeyDialogOpen(true);
    scheduleAfterMenuClose(() => {
      ignoreMenuDialogDismissRef.current = false;
    }, 100);
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

  const importCoexistenceNotice = liveAuthCoexistenceNotice(importLiveProbe, addAgentId, t);
  const oauthImportGate = liveAuthImportGate(
    importLiveProbe,
    importProbeLoading,
    addAgentId,
    t,
  );
  const apiKeyImportGate = liveApiKeyImportGate(
    importLiveProbe,
    importProbeLoading,
    addAgentId,
    t,
  );
  const importDialogMode = liveImportDialogMode(importLiveProbe);
  const activeImportGate = importDialogMode === 'api-key' ? apiKeyImportGate : oauthImportGate;

  const discoveryKind = liveAuthDiscoveryKind({
    poolState: pool.state,
    probe: discoveryProbe?.agentId === discoveryAgentId ? discoveryProbe : null,
    accounts: accountsForAgent(pool.accounts, discoveryAgentId),
    providers: providersForAgent(pool.providers, discoveryAgentId),
    accountsFailed: Boolean(pool.errors.accounts),
    providersFailed: Boolean(pool.errors.providers),
  });
  const showDiscoveryBanner =
    !discoveryLoading && !discoveryDismissed && !loginImportOpen && discoveryKind !== null;

  const confirmImportLogin = async () => {
    if (!activeImportGate.enabled) return;
    const coexistenceNotice = importCoexistenceNotice;
    setImportingAccount(true);
    try {
      const acc = await importCurrentLogin(addAgentId);
      setLoginImportOpen(false);
      toast({
        title: t('connections.import.toastOk'),
        description: coexistenceNotice
          ? t('connections.import.toastOkCoexist', { label: acc.label })
          : t('connections.import.toastOkDesc', { label: acc.label }),
        variant: 'success',
      });
      await poolReload().catch(() => {});
      setDiscoveryDismissed(true);
      await loadWallet();
      handleGuideSucceeded();
    } catch (e) {
      toast({
        title: t('connections.import.toastFail'),
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
        title: t('connections.delete.toastOk'),
        description: deleteConnectionToastDescription({ isCurrent: extras?.isCurrent === true }, t),
        variant: 'success',
      });
      await loadWallet();
      await poolReload().catch(() => {});
    } catch (e) {
      toast({
        title: t('connections.delete.toastFail'),
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
        <PageHeader
          title={t('connections.page.title')}
          description={t('connections.page.description')}
          descriptionTip={t('connections.page.descriptionTipLoading')}
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
          title={t('connections.page.title')}
          description={t('connections.page.description')}
          descriptionTip={t('connections.page.descriptionTipError')}
        />
        <ErrorState error={error} title={t('connections.page.agentStatusError')} onRetry={() => void reload()} />
      </div>
    );
  }

  if (!loading && installedIds.length === 0) {
    return (
      <div>
        <PageHeader
          title={t('connections.page.title')}
          description={t('connections.page.description')}
          descriptionTip={t('connections.page.descriptionTipEmpty')}
        />
        <EmptyState
          icon={Cable}
          title={t('connections.page.emptyTitle')}
          description={t('connections.page.emptyDesc')}
          actionLabel={t('connections.page.emptyAction')}
          onAction={() => navigate('/agents')}
        />
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title={t('connections.page.title')}
        description={
          visibleWallet
            ? t('connections.page.descriptionCount', { n: visibleWallet.tickets.length })
            : t('connections.page.descriptionKinds')
        }
        descriptionTip={t('connections.page.descriptionTip')}
        actions={
          <TicketAddMenu
            agents={buildTicketAddMenu(allowedAgents)}
            focusedAgentId={filterAgent === 'all' ? null : filterAgent}
            onImportLogin={(id) => openTicketAdd('import-login', id)}
            onAddKey={(id) => openTicketAdd('api-key', id)}
          />
        }
      />

      <div className={pageRhythm.chrome}>
        <AgentTabStrip
          showAll
          allLabel={t('kind.all')}
          value={filterAgent}
          onChange={setFilterAgent}
          agents={tabAgents}
          counts={agentCounts}
          countMode="defined"
          countTitle={(id, n) =>
            id === 'all'
              ? t('connections.page.countAll', { n })
              : t('connections.page.countAgent', { name: agentDisplayName(id), n })
          }
          emptyLabel={t('connections.page.emptyTitle')}
          aria-label={t('connections.page.filterAria')}
        />
      </div>

      {showDiscoveryBanner && discoveryKind ? (
        <div className={pageRhythm.lead}>
          <Notice
            tone="info"
            actionLabel={t('connections.discovery.action')}
            onAction={() => {
              setAddAgentId(discoveryAgentId);
              if (discoveryProbe?.agentId === discoveryAgentId) {
                setImportLiveProbe(discoveryProbe);
              }
              setLoginImportOpen(true);
            }}
            onDismiss={() => setDiscoveryDismissed(true)}
          >
            {discoveryKind === 'provider'
              ? t('connections.discovery.providerBanner', { name: agentDisplayName(discoveryAgentId) })
              : t('connections.discovery.accountBanner', { name: agentDisplayName(discoveryAgentId) })}
          </Notice>
        </div>
      ) : null}

      {resumeAgentId ? (
        <div className={pageRhythm.lead}>
          <Notice
            tone="info"
            actionLabel={t('connections.page.resumeAction')}
            onAction={() => navigate(buildResumeConnectUrl(resumeAgentId))}
          >
            {t('connections.page.resumeNotice')}
          </Notice>
        </div>
      ) : null}

      {walletError && !wallet ? (
        <ErrorState
          error={walletError}
          title={t('connections.page.walletError')}
          onRetry={() => void loadWallet()}
        />
      ) : (
        <>
          {walletError && wallet ? (
            <Notice
              className="mb-3 text-sm"
              tone="warning"
              actionLabel={t('chrome.error.retry')}
              onAction={() => void loadWallet()}
            >
              {t('connections.page.walletRefreshFailed')}
            </Notice>
          ) : null}
          <TicketWalletList
            wallet={visibleWallet}
            loading={walletLoading}
            highlightAgentId={highlightAgentId}
            agentFilterId={filterAgent === 'all' ? null : filterAgent}
            onShareTicket={handleShareTicket}
            onRouteTicket={handleRouteTicket}
            onSwitchTicket={handleSwitchTicket}
            onRefreshTicket={handleRefreshTicket}
            refreshingTicketId={refreshingTicketId}
            switchingTicketId={switchingTicketId}
            extrasForTicket={extrasForTicket}
            onEditTicket={handleEditTicket}
            onDeleteTicket={setDeleteTicket}
            onClearAgentFilter={() => setFilterAgent('all')}
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
        onOpenChange={(open) => {
          if (shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, open)) return;
          closeConfirmationOnOpenChange(open, importingAccount, () => setLoginImportOpen(false));
        }}
      >
        <DialogContent
          className="max-w-sm"
          hideClose={importingAccount}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(importingAccount, event)}
        >
          <DialogHeader>
            <DialogTitle>
              {importDialogMode === 'api-key'
                ? t('connections.import.apiKeyTitle')
                : t('connections.import.title')}
            </DialogTitle>
            <DialogDescription>
              {importDialogMode === 'api-key'
                ? t('connections.import.apiKeyDescription', { name: agentDisplayName(addAgentId) })
                : t('connections.import.description', { name: agentDisplayName(addAgentId) })}
            </DialogDescription>
          </DialogHeader>
          {importProbeLoading ? (
            <p className="text-xs text-muted">{t('connections.import.probing')}</p>
          ) : null}
          {!importProbeLoading && !activeImportGate.enabled && activeImportGate.reason ? (
            <Notice tone="warning">{activeImportGate.reason}</Notice>
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
              {t('common.cancel')}
            </Button>
            <Button
              disabled={importingAccount || !activeImportGate.enabled}
              onClick={() => void confirmImportLogin()}
            >
              {importingAccount
                ? t('connections.import.importing')
                : importDialogMode === 'api-key'
                  ? t('connections.import.apiKeyConfirm')
                  : t('connections.import.confirm')}
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
            <DialogTitle>{t('connections.delete.title')}</DialogTitle>
            <DialogDescription>
              {deleteTicket
                ? `${deleteTicket.label} · ${deleteConnectionDialogDescription({
                    isCurrent: extrasForTicket(deleteTicket)?.isCurrent === true,
                  }, t)}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={deleteBusy} onClick={() => setDeleteTicket(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={deleteBusy}
              onClick={() => void confirmDeleteTicket()}
            >
              {deleteBusy ? t('connections.delete.deleting') : t('connections.delete.confirm')}
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
          if (shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, v)) return;
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
