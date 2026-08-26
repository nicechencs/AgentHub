// Connections：全局票钱包（docs/connection-binding-model.md §5.2）
// AgentTabStrip 筛选；?agent= 高亮并把 Tab 落到该 Agent。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
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
  ticketIdFor,
  type TicketView,
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
  getTicketWalletSnapshot,
  providersForAgent,
  useConnectionPool,
  useTicketWallet,
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
import { TicketAddMenu, TicketDetailPanel, TicketWalletList } from './TicketWalletList';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  extrasFromPoolSource,
  filterTicketsByAgentUsage,
  filterWalletByExcludedAgents,
  findTicketPoolSource,
  scheduleAfterMenuClose,
  shouldIgnoreMenuDialogDismiss,
  ticketAddDialogState,
  ticketDetailEditLabel,
  type TicketAddKind,
} from './ticket-wallet-model';
import { useTicketBindActions } from './use-ticket-route-actions';
import {
  deleteConnectionDialogDescription,
  deleteConnectionToastDescription,
  liveAuthCoexistenceNotice,
  liveAuthImportGate,
  liveApiKeyImportGate,
  liveAuthDiscoveryKind,
  liveImportAction,
  liveImportDialogMode,
} from './connection-model';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
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
import { deleteProvider, importProviderLive, switchPreview, switchProvider } from '@/lib/api/provider';
import type { Account, Provider } from '@/lib/types';

type ConnectionInspect =
  | { kind: 'provider'; agentId: AgentId; mode: 'add' | 'edit'; provider: Provider | null }
  | { kind: 'account'; agentId: AgentId; account: Account }
  | { kind: 'detail'; ticketId: string }
  | { kind: 'connect'; entry: Extract<ConnectFlowEntry, { mode: 'for-source' }> };

function inspectActiveTicketId(target: ConnectionInspect | null): string | null {
  if (!target) return null;
  if (target.kind === 'detail') return target.ticketId;
  if (target.kind === 'connect') {
    return ticketIdFor(target.entry.source.kind, target.entry.source.id);
  }
  if (target.kind === 'provider' && target.mode === 'edit' && target.provider) {
    return ticketIdFor('provider', target.provider.id);
  }
  if (target.kind === 'account') {
    return ticketIdFor('account', target.account.id);
  }
  return null;
}

const CONNECTIONS_INSPECT_WIDTH_KEY = 'agenthub.connections.inspectWidth';

function parseAgentParam(raw: string | null, allowed: AgentId[]): AgentId | null {
  if (raw && allowed.includes(raw as AgentId)) return raw as AgentId;
  return null;
}

export default function ConnectionsPage() {
  const { t } = useI18n();
  const {
    installedIds,
    visibleIds,
    omittedIds,
    loading,
    state,
    error,
    reload,
  } = useInstalledAgents();
  const pool = useConnectionPool();
  const navigate = useNavigate();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();

  const allowedAgents = installedIds.length > 0 || !loading ? installedIds : visibleIds;
  const omittedSet = useMemo(() => new Set(omittedIds), [omittedIds]);
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

  const {
    wallet,
    error: walletError,
    state: walletState,
    reload: walletReload,
    ensureLoaded: walletEnsureLoaded,
  } = useTicketWallet();
  const walletLoading =
    (walletState === 'idle' || walletState === 'loading') && wallet == null;
  const connectDeps = useMemo(() => createDefaultConnectFlowDeps(), []);

  /** Agent context for add/import dialogs (deep-link or picker). */
  const [addAgentId, setAddAgentId] = useState<AgentId>(
    () => highlightAgentId ?? allowedAgents[0] ?? 'claude',
  );
  const inspect = useSideSplit<ConnectionInspect>({ storageKey: CONNECTIONS_INSPECT_WIDTH_KEY });
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
    if (filterAgent === 'all' || loading) return;
    if (!installedIds.includes(filterAgent)) {
      setFilterAgent('all');
    }
  }, [filterAgent, installedIds, loading]);

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
    if (walletState === 'idle') void walletEnsureLoaded();
  }, [walletEnsureLoaded, walletState]);

  const visibleWallet = useMemo(
    () => filterWalletByExcludedAgents(wallet, omittedSet),
    [omittedSet, wallet],
  );

  const tabAgentIds = allowedAgents;
  const tabAgents = useMemo(
    () => tabAgentIds.map((id) => resolveAgentMeta(id)),
    [tabAgentIds],
  );

  const agentCounts = useMemo(() => {
    const tickets = visibleWallet?.tickets ?? [];
    const counts: Partial<Record<AgentTabId, number>> = { all: tickets.length };
    if (!visibleWallet) {
      for (const id of tabAgentIds) counts[id] = 0;
      return counts;
    }
    for (const id of tabAgentIds) {
      counts[id] = filterTicketsByAgentUsage(visibleWallet, tickets, id).length;
    }
    return counts;
  }, [tabAgentIds, visibleWallet]);

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
    const allowed = installedIds.length > 0 || !loading ? installedIds : visibleIds;
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
  }, [installedIds, loading, visibleIds, searchParams, setSearchParams]);

  useEffect(() => {
    const intent = pendingGuide?.intent ?? null;
    if (!intent) return;
    if (intent === 'add-key') {
      guideOpenedApiKeyRef.current = true;
      inspect.open({
        kind: 'provider',
        mode: 'add',
        agentId: addAgentId,
        provider: null,
      });
      setPendingGuide(null);
      return;
    }
    if (intent === 'import-login') {
      setLoginImportOpen(true);
      setPendingGuide(null);
    }
  }, [pendingGuide, addAgentId, inspect.open]);

  const handleGuideSucceeded = useCallback(() => {
    const resume = pendingGuide?.resumeAgentId ?? resumeAgentId;
    setPendingGuide(null);
    if (resume) navigate(buildResumeConnectUrl(resume));
  }, [navigate, pendingGuide, resumeAgentId]);

  const openConnectForTicket = useCallback((ticket: TicketView, purpose: 'share' | 'route') => {
    setLoginImportOpen(false);
    if (
      inspect.target?.kind === 'connect'
      && inspect.target.entry.purpose === purpose
      && inspect.target.entry.source.kind === ticket.sourceKind
      && inspect.target.entry.source.id === ticket.sourceId
    ) {
      // Repeated activation is idempotent. The share/route action is an
      // opener, so clicking it again must not collapse the already-open pane.
      return;
    }
    inspect.open({
      kind: 'connect',
      entry: {
        mode: 'for-source',
        source: { kind: ticket.sourceKind, id: ticket.sourceId },
        purpose,
      },
    });
  }, [inspect.close, inspect.open, inspect.target]);

  const handleShareTicket = useCallback((ticket: TicketView) => {
    openConnectForTicket(ticket, 'share');
  }, [openConnectForTicket]);

  const handleRouteTicket = useCallback((ticket: TicketView) => {
    openConnectForTicket(ticket, 'route');
  }, [openConnectForTicket]);

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

  const { shareActionForTicket, routeActionForTicket } = useTicketBindActions({
    tickets: visibleWallet?.tickets ?? [],
    accounts: pool.accounts,
    providers: pool.providers,
    hiddenIds: omittedIds,
    poolReady: pool.state === 'ready' || pool.state === 'partial',
    deps: connectDeps,
    t,
  });

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
    ignoreMenuDialogDismissRef.current = true;
    if (next.loginImportOpen) {
      inspect.close();
      setLoginImportOpen(true);
    }
    if (next.apiKeyDialogOpen) {
      setLoginImportOpen(false);
      inspect.open({
        kind: 'provider',
        mode: 'add',
        agentId: next.addAgentId,
        provider: null,
      });
    }
    scheduleAfterMenuClose(() => {
      ignoreMenuDialogDismissRef.current = false;
    }, 100);
  }, [inspect.close, inspect.open]);

  const handleEditTicket = useCallback(
    (ticket: TicketView) => {
      const source = findTicketPoolSource(ticket, pool.accounts, pool.providers);
      setLoginImportOpen(false);
      if (source.provider) {
        inspect.open({
          kind: 'provider',
          mode: 'edit',
          agentId: source.provider.agentId,
          provider: source.provider,
        });
        return;
      }
      if (source.account?.kind === 'apikey') {
        inspect.open({
          kind: 'account',
          agentId: source.account.agentId,
          account: source.account,
        });
      }
    },
    [inspect.open, pool.accounts, pool.providers],
  );

  const handleShowDetail = useCallback(
    (ticket: TicketView) => {
      setLoginImportOpen(false);
      if (inspect.target?.kind === 'detail' && inspect.target.ticketId === ticket.id) {
        inspect.close();
        return;
      }
      inspect.open({ kind: 'detail', ticketId: ticket.id });
    },
    [inspect.close, inspect.open, inspect.target],
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
      const imported =
        liveImportAction(importDialogMode) === 'provider'
          ? await importProviderLive(addAgentId)
          : await importCurrentLogin(addAgentId);
      const label = 'label' in imported ? imported.label : imported.name;
      setLoginImportOpen(false);
      toast({
        title: t('connections.import.toastOk'),
        description: coexistenceNotice
          ? t('connections.import.toastOkCoexist', { label })
          : t('connections.import.toastOkDesc', { label }),
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

  const inspectTarget = inspect.target;
  const detailTicket = inspectTarget?.kind === 'detail'
    ? visibleWallet?.tickets.find((ticket) => ticket.id === inspectTarget.ticketId) ?? null
    : null;
  const detailBindings = detailTicket && visibleWallet
    ? visibleWallet.bindings.filter((binding) => binding.ticketId === detailTicket.id)
    : [];
  const inspectPanel =
    inspectTarget?.kind === 'provider' ? (
      <ProviderEditDialog
        asPanel
        open
        width={inspect.paneWidth}
        agentId={inspectTarget.agentId}
        mode={inspectTarget.mode}
        provider={inspectTarget.provider}
        onOpenChange={(v) => {
          if (shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, v)) return;
          if (!v) {
            guideOpenedApiKeyRef.current = false;
            inspect.close();
          }
        }}
        onSaved={() => {
          const fromGuide = guideOpenedApiKeyRef.current;
          guideOpenedApiKeyRef.current = false;
          inspect.close();
          void loadWallet();
          void poolReload();
          if (fromGuide) handleGuideSucceeded();
        }}
      />
    ) : inspectTarget?.kind === 'account' ? (
      <ApiKeyAccountDialog
        asPanel
        open
        width={inspect.paneWidth}
        agentId={inspectTarget.agentId}
        mode="edit"
        account={inspectTarget.account}
        onOpenChange={(v) => {
          if (!v) inspect.close();
        }}
        onSaved={() => {
          inspect.close();
          void loadWallet();
          void poolReload();
        }}
      />
    ) : inspectTarget?.kind === 'detail' && detailTicket ? (
      <TicketDetailPanel
        id={`ticket-detail-${detailTicket.id}`}
        asPanel
        open
        width={inspect.paneWidth}
        ticket={detailTicket}
        extras={extrasForTicket(detailTicket)}
        bindings={detailBindings}
        refreshing={refreshingTicketId === detailTicket.id}
        refreshLocked={refreshingTicketId !== null}
        onRefresh={
          extrasForTicket(detailTicket)?.oauthAction
            ? () => void handleRefreshTicket(detailTicket)
            : undefined
        }
        onDelete={() => setDeleteTicket(detailTicket)}
        onEdit={ticketDetailEditLabel(extrasForTicket(detailTicket), t)
          ? () => handleEditTicket(detailTicket)
          : undefined}
        onOpenChange={(next) => { if (!next) inspect.close(); }}
      />
    ) : inspectTarget?.kind === 'connect' ? (
      <ConnectFlowDialog
        asPanel
        width={inspect.paneWidth}
        entry={inspectTarget.entry}
        deps={connectDeps}
        onClose={() => inspect.close()}
        onConnectionChanged={handleConnectionChanged}
        onNavigate={(to) => navigate(to)}
      />
    ) : null;

  const trashDock = (
    <ConnectionTrashButton onChanged={() => void loadWallet()} />
  );

  if (loading) {
    return (
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
        listFooter={trashDock}
        header={(
          <PageHeader
            size="compact"
            title={t('connections.page.title')}
            description={t('connections.page.description')}
            descriptionTip={t('connections.page.descriptionTipLoading')}
          />
        )}
      >
        <div className={pageRhythm.chrome}>
          <ListSkeleton rows={4} />
        </div>
      </WorkbenchSplitPage>
    );
  }

  if (state === 'error') {
    return (
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
        listFooter={trashDock}
        header={(
          <PageHeader
            size="compact"
            title={t('connections.page.title')}
            description={t('connections.page.description')}
            descriptionTip={t('connections.page.descriptionTipError')}
          />
        )}
      >
        <ErrorState error={error} title={t('connections.page.agentStatusError')} onRetry={() => void reload()} />
      </WorkbenchSplitPage>
    );
  }

  if (!loading && installedIds.length === 0) {
    return (
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
        listFooter={trashDock}
        header={(
          <PageHeader
            size="compact"
            title={t('connections.page.title')}
            description={t('connections.page.description')}
            descriptionTip={t('connections.page.descriptionTipEmpty')}
          />
        )}
      >
        <EmptyState
          icon={Cable}
          title={t('connections.page.emptyTitle')}
          description={t('connections.page.emptyDesc')}
          actionLabel={t('connections.page.emptyAction')}
          onAction={() => navigate('/agents')}
        />
      </WorkbenchSplitPage>
    );
  }

  return (
    <>
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={inspectPanel}
      listFooter={trashDock}
      header={(
        <PageHeader
          size="compact"
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
      )}
    >

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
            shareActionForTicket={shareActionForTicket}
            routeActionForTicket={routeActionForTicket}
            onSwitchTicket={handleSwitchTicket}
            switchingTicketId={switchingTicketId}
            extrasForTicket={extrasForTicket}
            onEditTicket={handleEditTicket}
            onDeleteTicket={setDeleteTicket}
            onShowDetail={handleShowDetail}
            activeTicketId={inspectActiveTicketId(inspectTarget)}
            onClearAgentFilter={() => setFilterAgent('all')}
            installedAgentIds={allowedAgents}
            onAddKey={(id) => openTicketAdd('api-key', id)}
            onImportLogin={(id) => openTicketAdd('import-login', id)}
          />
        </>
      )}
    </WorkbenchSplitPage>

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
              variant="secondary"
              disabled={importingAccount}
              onClick={() => setLoginImportOpen(false)}
            >
              {t('common.cancel')}
            </Button>
            <Hint label={!activeImportGate.enabled ? activeImportGate.reason : undefined}>
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
            </Hint>
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
            <Button variant="secondary" disabled={deleteBusy} onClick={() => setDeleteTicket(null)}>
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
    </>
  );
}
