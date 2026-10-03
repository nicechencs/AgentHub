// Connections：全局票钱包（docs/concepts/connections-and-routing.md）
// AgentTabStrip 筛选；?agent= 高亮并把 Tab 落到该 Agent。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { Cable } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { followInspectOpen } from '@/components/layout/inspect-follow';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName, resolveAgentMeta } from '@/config/agents';
import {
  ticketIdFor,
  type TicketView,
} from '@/lib/api/tickets';
import { OAuthFlowDialog } from '@/components/connect/OAuthFlowDialog';
import { officialLoginDiscovery } from '@/components/connect/official-login-discovery';
import { officialLoginSuccessView } from '@/lib/backend/contracts/official-login-session';
import { openExternalLink } from '@/lib/open-external';
import { createConnectionsOfficialLoginPersistence } from './official-login-persistence';
import {
  buildResumeConnectUrl,
  consumeConnectIntent,
  parseResumeAgentId,
  readConnectApiKeyDraft,
  readConnectGuide,
  type ConnectApiKeyDraft,
  type ConnectGuide,
} from '@/lib/connect-flow/connect-intent';
import {
  accountsForAgent,
  getTicketWalletSnapshot,
  providersForAgent,
  useConnectionInventory,
  useTicketWallet,
} from '@/app/runtime';
import { isAuthorizationManagementBlocked } from '@/lib/capability';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  oauthListAction,
  oauthListActionProbesQuota,
} from '@/lib/backend/contracts/account-actions';
import type { AgentKey } from '@/lib/types';
import { ApiKeyAccountDialog } from '@/components/connections/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/components/connections/ProviderEditDialog';
import { ConnectionTrashButton } from './ConnectionTrashButton';
import { importLoginErrorNotice, importLoginReportNotice } from './import-login-notice';
import { TicketAddMenu, TicketDetailPanel, TicketWalletList } from './TicketWalletList';
import {
  activeBindingForAgent,
  buildTicketAddMenu,
  extrasFromPoolSource,
  filterWalletByExcludedAgents,
  findTicketPoolSource,
  officialDetailQuotaNeedsProbe,
  scheduleAfterMenuClose,
  showsCatalogUnapply,
  shouldIgnoreMenuDialogDismiss,
  ticketAddDialogState,
  ticketDetailEditLabel,
  type TicketAddKind,
} from './ticket-wallet-model';
import { useOAuthLoginAgents } from './use-oauth-login-agents';
import { useConnectionImportProbe } from './use-connection-import-probe';
import { useConnectionPageActions } from './use-connection-page-actions';
import { usePiDefaultModel } from './use-pi-default-model';
import {
  deleteConnectionDialogDescription,
  deleteCurrentSwitchTargets,
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
  importCurrentLogin,
  importCurrentLoginWithReport,
  probeLiveAuth,
  refreshQuota,
  refreshToken,
  switchAccount,
  type LiveAuthProbe,
} from '@/lib/api/account';
import { listConnectionUsage } from '@/lib/api/usage';
import type { ConnectionUsageSummary } from '@/lib/backend/contracts/usage-types';
import { importProviderLive } from '@/lib/api/provider';
import { getSettings } from '@/lib/api/settings';
import type { Account, Provider } from '@/lib/types';
import type { ImportLoginReport } from '@/lib/api/account';
import { StorageKey } from '@/lib/ui-preferences';
import {
  canAutoImportProbe,
  planLocalLoginAutoImport,
  resolveAutoImportLocalLogin,
  shouldRememberAutoImportAttempt,
  showConnectionsImportLoginAction,
} from './local-login-auto-import';

type ConnectionInspect =
  | { kind: 'provider'; agentId: AgentKey; mode: 'add' | 'edit'; provider: Provider | null }
  | { kind: 'account'; agentId: AgentKey; account: Account | null }
  | { kind: 'detail'; ticketId: string };

function inspectActiveTicketId(target: ConnectionInspect | null): string | null {
  if (!target) return null;
  if (target.kind === 'detail') return target.ticketId;
  if (target.kind === 'provider' && target.mode === 'edit' && target.provider) {
    return ticketIdFor('provider', target.provider.id);
  }
  if (target.kind === 'account' && target.account) {
    return ticketIdFor('account', target.account.id);
  }
  return null;
}

const CONNECTIONS_INSPECT_WIDTH_KEY = StorageKey.connectionsInspectWidth;

function parseAgentParam(raw: string | null, allowed: AgentKey[]): AgentKey | null {
  if (raw && allowed.includes(raw as AgentKey)) return raw as AgentKey;
  return null;
}

export default function ConnectionsPage() {
  const { t } = useI18n();
  const {
    installedIds,
    installedAgents,
    visibleIds,
    omittedIds,
    loading,
    state,
    error,
    reload,
  } = useInstalledAgents();
  const pool = useConnectionInventory();
  const navigate = useNavigate();
  const location = useLocation();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const [apiKeyDraft, setApiKeyDraft] = useState<ConnectApiKeyDraft | null>(null);

  const allowedAgents = installedIds.length > 0 || !loading ? installedIds : visibleIds;
  const authBlockedIds = useMemo(
    () => allowedAgents.filter((id) => {
      const caps = installedAgents.find((agent) => agent.id === id)?.capabilities;
      return isAuthorizationManagementBlocked(id, caps);
    }),
    [allowedAgents, installedAgents],
  );
  const authBlockedSet = useMemo(() => new Set(authBlockedIds), [authBlockedIds]);
  const manageAuthAgentIds = useMemo(
    () => allowedAgents.filter((id) => !authBlockedSet.has(id)),
    [allowedAgents, authBlockedSet],
  );
  const oauthLoginAgents = useOAuthLoginAgents(manageAuthAgentIds);
  const omittedSet = useMemo(
    () => new Set([...omittedIds, ...authBlockedIds]),
    [omittedIds, authBlockedIds],
  );
  const highlightAgentId = parseAgentParam(searchParams.get('agent'), manageAuthAgentIds);
  const resumeAgentId = parseResumeAgentId(searchParams.get('resume'), allowedAgents);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>(highlightAgentId ?? 'all');
  const [refreshingTicketId, setRefreshingTicketId] = useState<string | null>(null);
  const refreshGen = useRef(0);
  const refreshInFlightRef = useRef(false);

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
  const [connectionUsage, setConnectionUsage] = useState<Map<string, ConnectionUsageSummary>>(
    () => new Map(),
  );

  useEffect(() => {
    let cancelled = false;
    void listConnectionUsage()
      .then((rows) => {
        if (cancelled) return;
        setConnectionUsage(new Map(rows.map((row) => [row.ticketId, row])));
      })
      .catch(() => {
        if (!cancelled) setConnectionUsage(new Map());
      });
    return () => {
      cancelled = true;
    };
  }, [wallet]);

  /** Agent context for add/import dialogs (deep-link or picker). */
  const [addAgentId, setAddAgentId] = useState<AgentKey>(
    () => highlightAgentId ?? allowedAgents[0] ?? 'claude',
  );
  const inspect = useSideSplit<ConnectionInspect>({ storageKey: CONNECTIONS_INSPECT_WIDTH_KEY });
  const [oauthOpen, setOauthOpen] = useState(false);
  const oauthAccountRef = useRef<Account | null>(null);
  const oauthPersistence = useMemo(() => createConnectionsOfficialLoginPersistence({
    onAccount: (account) => {
      oauthAccountRef.current = account;
    },
  }), []);
  const [discoveryProbe, setDiscoveryProbe] = useState<LiveAuthProbe | null>(null);
  const [discoveryLoading, setDiscoveryLoading] = useState(false);
  const [discoveryDismissed, setDiscoveryDismissed] = useState(false);
  const discoveryProbeGen = useRef(0);
  const [autoImportLocalLogin, setAutoImportLocalLogin] = useState(true);
  const autoImportTriedRef = useRef(new Set<string>());
  const autoImportGen = useRef(0);
  const {
    loginImportOpen,
    setLoginImportOpen,
    importLiveProbe,
    setImportLiveProbe,
    importProbeLoading,
    importingAccount,
    setImportingAccount,
  } = useConnectionImportProbe({ addAgentId, discoveryProbe });
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
    if (!installedIds.includes(filterAgent) || authBlockedSet.has(filterAgent)) {
      setFilterAgent('all');
    }
  }, [authBlockedSet, filterAgent, installedIds, loading]);

  useEffect(() => {
    if (!authBlockedSet.has(addAgentId)) return;
    const next = manageAuthAgentIds[0];
    if (next) setAddAgentId(next);
  }, [addAgentId, authBlockedSet, manageAuthAgentIds]);

  const discoveryAgentId: AgentKey = filterAgent === 'all' ? addAgentId : filterAgent;

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

  useEffect(() => {
    let cancelled = false;
    void getSettings()
      .then((settings) => {
        if (!cancelled) setAutoImportLocalLogin(resolveAutoImportLocalLogin(settings.autoImportLocalLogin));
      })
      .catch(() => {
        if (!cancelled) setAutoImportLocalLogin(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const includeImportLogin = showConnectionsImportLoginAction(autoImportLocalLogin);

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
      counts[id] = tickets.filter((ticket) => ticket.agentId === id).length;
    }
    return counts;
  }, [tabAgentIds, visibleWallet]);

  const poolReload = pool.reload;

  useEffect(() => {
    if (!resolveAutoImportLocalLogin(autoImportLocalLogin)) return;
    if (loading) return;
    if (pool.state !== 'ready' && pool.state !== 'partial') return;
    if (loginImportOpen || pendingGuide?.intent === 'import-login') return;
    const pending = planLocalLoginAutoImport({
      autoImportLocalLogin,
      agentIds: manageAuthAgentIds,
      alreadyTried: autoImportTriedRef.current,
    });
    if (pending.length === 0) return;

    const generation = ++autoImportGen.current;
    void (async () => {
      const importedLabels: string[] = [];
      let lastError: string | null = null;
      for (const agentId of pending) {
        if (autoImportGen.current !== generation) return;
        if (autoImportTriedRef.current.has(agentId)) continue;
        let probe: LiveAuthProbe | null = null;
        let probeOk = false;
        try {
          probe = await probeLiveAuth(agentId);
          probeOk = true;
        } catch {
          probe = null;
        }
        if (autoImportGen.current !== generation) return;
        const accountsFailed = Boolean(pool.errors.accounts);
        const providersFailed = Boolean(pool.errors.providers);
        if (!shouldRememberAutoImportAttempt({
          probeOk,
          poolState: pool.state,
          probe,
          accountsFailed,
          providersFailed,
        })) {
          continue;
        }
        autoImportTriedRef.current.add(agentId);
        if (!canAutoImportProbe({
          agentId,
          poolState: pool.state,
          probe,
          accounts: accountsForAgent(pool.accounts, agentId),
          providers: providersForAgent(pool.providers, agentId),
          accountsFailed,
          providersFailed,
        })) {
          continue;
        }
        try {
          let label: string;
          if (liveImportAction(liveImportDialogMode(probe), agentId) === 'provider') {
            label = (await importProviderLive(agentId)).name;
          } else {
            // Logins left in the recycle bin stay quiet here.
            const report = await importCurrentLoginWithReport(agentId);
            if (autoImportGen.current !== generation) return;
            if (!report.account) continue;
            label = report.account.label;
          }
          if (autoImportGen.current !== generation) return;
          importedLabels.push(label);
        } catch (e) {
          lastError = e instanceof Error ? e.message : String(e);
        }
      }
      if (autoImportGen.current !== generation) return;
      if (importedLabels.length > 0) {
        toast({
          title: importedLabels.length === 1
            ? t('connections.import.toastOk')
            : t('connections.import.toastOkMany', { n: importedLabels.length }),
          description: importedLabels.length === 1
            ? t('connections.import.toastOkDesc', { label: importedLabels[0] })
            : undefined,
          variant: 'success',
        });
        await poolReload().catch(() => {});
        await loadWallet();
      } else if (lastError) {
        toast({
          title: t('connections.import.toastFail'),
          description: lastError,
          variant: 'danger',
        });
      }
    })();
  }, [
    autoImportLocalLogin,
    loading,
    loginImportOpen,
    loadWallet,
    manageAuthAgentIds,
    pendingGuide,
    pool.accounts,
    pool.errors.accounts,
    pool.errors.providers,
    pool.providers,
    pool.state,
    poolReload,
    t,
    toast,
  ]);

  const handleTrashChanged = useCallback(() => {
    void Promise.all([loadWallet(), poolReload().catch(() => {})]);
  }, [loadWallet, poolReload]);

  useEffect(() => {
    const draft = readConnectApiKeyDraft(location.state);
    if (draft) setApiKeyDraft(draft);
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
  }, [installedIds, loading, visibleIds, location.state, searchParams, setSearchParams]);

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
    if (intent === 'oauth') {
      setOauthOpen(true);
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
      if (e instanceof Error && e.name === 'OauthFileSyncPending') {
        toast({
          title: t('connections.list.refreshPartial'),
          description: e.message,
          variant: 'danger',
        });
        await poolReload().catch(() => {});
        await loadWallet();
        return;
      }
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
        const extras = extrasFromPoolSource(
          ticket,
          findTicketPoolSource(ticket, pool.accounts, pool.providers),
          t,
          tabCurrentTicketId,
        );
        const usage = connectionUsage.get(ticket.id);
        if (usage) {
          extras.tokenInput = usage.inputTokens;
          extras.tokenOutput = usage.outputTokens;
          if (usage.lastUsedAt) {
            extras.tokenLastUsedAt = usage.lastUsedAt;
            extras.lastUsedAt = usage.lastUsedAt;
          }
        }
        return extras;
      } catch {
        return null;
      }
    },
    [connectionUsage, filterAgent, pool.accounts, pool.providers, t, wallet],
  );

  const {
    switchingTicketId,
    handleSwitchTicket,
    handleRemoveFromCatalog,
    deleteTicket,
    setDeleteTicket,
    deleteBusy,
    confirmDeleteTicket,
  } = useConnectionPageActions({
    filterAgent,
    wallet,
    extrasForTicket,
    loadWallet,
    poolReload,
  });

  const openTicketAdd = useCallback((kind: TicketAddKind, agentId: AgentKey) => {
    const next = ticketAddDialogState(kind, agentId);
    setAddAgentId(next.addAgentId);
    ignoreMenuDialogDismissRef.current = true;
    if (next.loginImportOpen) {
      inspect.close();
      setOauthOpen(false);
      setLoginImportOpen(true);
    }
    if (next.oauthDialogOpen) {
      inspect.close();
      setLoginImportOpen(false);
      setOauthOpen(true);
    }
    if (next.apiKeyDialogOpen) {
      setLoginImportOpen(false);
      setOauthOpen(false);
      // WorkBuddy custom models are catalog rows (account + models.json),
      // not a single provider snapshot that would replace the live API key.
      if (next.addAgentId === 'workbuddy') {
        inspect.open({
          kind: 'account',
          agentId: next.addAgentId,
          account: null,
        });
      } else {
        inspect.open({
          kind: 'provider',
          mode: 'add',
          agentId: next.addAgentId,
          provider: null,
        });
      }
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
      inspect.open({ kind: 'detail', ticketId: ticket.id });
    },
    [inspect.open],
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
    includeImportLogin
    && !discoveryLoading
    && !discoveryDismissed
    && !loginImportOpen
    && discoveryKind !== null;

  const confirmImportLogin = async () => {
    if (!activeImportGate.enabled) return;
    const coexistenceNotice = importCoexistenceNotice;
    setImportingAccount(true);
    try {
      let label: string;
      let report: ImportLoginReport | null = null;
      if (liveImportAction(importDialogMode, addAgentId) === 'provider') {
        label = (await importProviderLive(addAgentId)).name;
      } else {
        report = await importCurrentLoginWithReport(addAgentId);
        label = report.account?.label ?? '';
      }
      setLoginImportOpen(false);
      const baseDescription = coexistenceNotice
        ? t('connections.import.toastOkCoexist', { label })
        : t('connections.import.toastOkDesc', { label });
      if (report) {
        const notice = importLoginReportNotice(report, baseDescription, t);
        toast({
          title: notice.title,
          description: notice.description,
          variant: notice.variant,
        });
      } else {
        toast({
          title: t('connections.import.toastOk'),
          description: baseDescription,
          variant: 'success',
        });
      }
      await poolReload().catch(() => {});
      setDiscoveryDismissed(true);
      await loadWallet();
      if (!report || report.account) handleGuideSucceeded();
    } catch (e) {
      const notice = importLoginErrorNotice(e, t);
      toast({
        title: notice.title,
        description: notice.description,
        variant: notice.variant,
      });
    } finally {
      setImportingAccount(false);
    }
  };

  const inspectTarget = inspect.target;
  const detailTicket = inspectTarget?.kind === 'detail'
    ? visibleWallet?.tickets.find((ticket) => ticket.id === inspectTarget.ticketId) ?? null
    : null;
  const detailExtras = detailTicket ? extrasForTicket(detailTicket) : null;
  const detailCanUnapply = Boolean(
    detailTicket
    && detailTicket.agentId === 'pi'
    && detailTicket.sourceKind === 'provider'
    && showsCatalogUnapply(
      resolveAgentMeta(detailTicket.agentId).occupancy,
      detailExtras?.isCurrent,
      detailExtras?.inList,
    )
  );
  const piDefault = usePiDefaultModel({
    ticket: detailTicket,
    isCurrent: detailTicket ? extrasForTicket(detailTicket)?.isCurrent === true : false,
  });
  const probedQuotaTicketIds = useRef(new Set<string>());
  useEffect(() => {
    if (!detailTicket) return;
    const extras = extrasForTicket(detailTicket);
    if (!officialDetailQuotaNeedsProbe(extras)) return;
    if (probedQuotaTicketIds.current.has(detailTicket.id)) return;
    const source = findTicketPoolSource(detailTicket, pool.accounts, pool.providers);
    const account = source.account;
    if (!account || account.kind !== 'oauth') return;
    probedQuotaTicketIds.current.add(detailTicket.id);
    void refreshQuota(account.agentId, account.id)
      .then(() => Promise.all([poolReload().catch(() => {}), loadWallet()]))
      .catch(() => undefined);
  }, [detailTicket, extrasForTicket, loadWallet, pool.accounts, pool.providers, poolReload]);
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
        initialBaseUrl={inspectTarget.mode === 'add' ? apiKeyDraft?.baseUrl : undefined}
        initialApiKey={inspectTarget.mode === 'add' ? apiKeyDraft?.apiKey : undefined}
        initialModel={inspectTarget.mode === 'add' ? apiKeyDraft?.model : undefined}
        initialPiApi={inspectTarget.mode === 'add' ? apiKeyDraft?.piApi : undefined}
        compactGrokApiBackend={inspectTarget.mode === 'add' ? apiKeyDraft?.apiBackend : undefined}
        onOpenChange={(v) => {
          if (shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, v)) return;
          if (!v) {
            guideOpenedApiKeyRef.current = false;
            setApiKeyDraft(null);
            inspect.close();
          }
        }}
        onSaved={() => {
          const fromGuide = guideOpenedApiKeyRef.current;
          guideOpenedApiKeyRef.current = false;
          setApiKeyDraft(null);
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
        mode={inspectTarget.account ? 'edit' : 'add'}
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
        extras={detailExtras}
        bindings={detailBindings}
        refreshing={refreshingTicketId === detailTicket.id}
        refreshLocked={refreshingTicketId !== null}
        onRefresh={
          extrasForTicket(detailTicket)?.oauthAction
            ? () => void handleRefreshTicket(detailTicket)
            : undefined
        }
        onDelete={() => setDeleteTicket(detailTicket)}
        onRemoveFromCatalog={detailCanUnapply
          ? () => void handleRemoveFromCatalog(detailTicket)
          : undefined}
        removeFromCatalogBusy={switchingTicketId === detailTicket.id}
        onEdit={ticketDetailEditLabel(extrasForTicket(detailTicket), t)
          ? () => handleEditTicket(detailTicket)
          : undefined}
        piDefaultModel={piDefault.view}
        onSwitchPiDefaultModel={(model) => void piDefault.switchModel(model)}
        onOpenChange={(next) => { if (!next) inspect.close(); }}
      />
    ) : null;

  const trashDock = (
    <ConnectionTrashButton onChanged={handleTrashChanged} />
  );

  if (loading) {
    return (
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
        listFooter={trashDock}
      >
        <PageHeader
          title={t('connections.page.title')}
          description={t('connections.page.description')}
          descriptionTip={t('connections.page.descriptionTipLoading')}
        />
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
      >
        <PageHeader
          title={t('connections.page.title')}
          description={t('connections.page.description')}
          descriptionTip={t('connections.page.descriptionTipError')}
        />
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
      >
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
      </WorkbenchSplitPage>
    );
  }


  const deleteIsCurrent = deleteTicket
    ? extrasForTicket(deleteTicket)?.isCurrent === true
    : false;
  const deleteIsCurrentPiProvider = Boolean(
    deleteTicket
    && deleteTicket.agentId === 'pi'
    && deleteTicket.sourceKind === 'provider'
    && deleteIsCurrent,
  );
  const deleteSwitchTargets = deleteTicket && deleteIsCurrent && !deleteIsCurrentPiProvider && wallet
    ? deleteCurrentSwitchTargets(
      deleteTicket,
      wallet.tickets,
      (ticket) => extrasForTicket(ticket)?.isCurrent === true,
    )
    : [];
  return (
    <>
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={inspectPanel}
      listFooter={trashDock}
    >
      <PageHeader
        title={t('connections.page.title')}
        description={
          visibleWallet
            ? t('connections.page.descriptionCount', { n: visibleWallet.tickets.length })
            : t('connections.page.descriptionKinds')
        }
        descriptionTip={t('connections.page.descriptionTip')}
      />
      <div className={pageRhythm.chromeRow} data-help="page-chrome">
        <AgentTabStrip
          showAll
          allLabel={t('kind.all')}
          value={filterAgent}
          onChange={setFilterAgent}
          agents={tabAgents}
          disabled={authBlockedIds}
          disabledReason={t('connections.capability.authUnsupported')}
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
        <div className={pageRhythm.chromeActions}>
          <TicketAddMenu
            agents={buildTicketAddMenu(manageAuthAgentIds, oauthLoginAgents, includeImportLogin)}
            focusedAgentId={filterAgent === 'all' ? null : filterAgent}
            onImportLogin={(id) => openTicketAdd('import-login', id)}
            onOauth={(id) => openTicketAdd('oauth', id)}
            onAddKey={(id) => openTicketAdd('api-key', id)}
            importDetectedAgentId={discoveryKind ? discoveryAgentId : null}
          />
        </div>
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
            onSwitchTicket={(ticket) => {
              void (async () => {
                await handleSwitchTicket(ticket);
                if (ticket.agentId === 'pi') await piDefault.reload();
              })();
            }}
            onRemoveFromCatalog={(ticket) => void handleRemoveFromCatalog(ticket)}
            switchingTicketId={switchingTicketId}
            extrasForTicket={extrasForTicket}
            onEditTicket={handleEditTicket}
            onDeleteTicket={setDeleteTicket}
            onShowDetail={handleShowDetail}
            onFollowDetail={followInspectOpen(
              Boolean(inspect.expanded && inspectTarget?.kind === 'detail'),
              handleShowDetail,
            )}
            activeTicketId={inspectActiveTicketId(inspectTarget)}
            onClearAgentFilter={() => setFilterAgent('all')}
            installedAgentIds={manageAuthAgentIds}
            oauthLoginAgents={oauthLoginAgents}
            onAddKey={(id) => openTicketAdd('api-key', id)}
            onImportLogin={(id) => openTicketAdd('import-login', id)}
            onOauth={(id) => openTicketAdd('oauth', id)}
            includeImportLogin={includeImportLogin}
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
            <Notice tone="warning">
              <details>
                <summary className="cursor-pointer">
                  {t('connections.list.coexistSummary')}{' '}
                  <span className="text-muted">{t('connections.list.coexistDetails')}</span>
                </summary>
                <p className="mt-1">{importCoexistenceNotice}</p>
              </details>
            </Notice>
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

      <OAuthFlowDialog
        agentId={addAgentId}
        agentName={agentDisplayName(addAgentId)}
        open={oauthOpen}
        persistence={oauthPersistence}
        discovery={officialLoginDiscovery}
        openLink={openExternalLink}
        describeSuccess={() => (
          oauthAccountRef.current ? officialLoginSuccessView(oauthAccountRef.current) : null
        )}
        onOpenChange={(open) => {
          if (shouldIgnoreMenuDialogDismiss(ignoreMenuDialogDismissRef.current, open)) return;
          setOauthOpen(open);
        }}
        onStored={() => {
          void loadWallet();
        }}
        onCompleted={(result) => {
          setOauthOpen(false);
          void (async () => {
            try {
              await switchAccount(result.source.agentId, result.source.sourceId);
              toast({ title: t('connect.oauth.success'), variant: 'success' });
              await poolReload().catch(() => {});
              await loadWallet();
            } catch (e) {
              toast({
                title: t('connect.oauth.failedTitle'),
                description: e instanceof Error ? e.message : String(e),
                variant: 'danger',
              });
            }
          })();
        }}
      />

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
                ? `${deleteTicket.label} · ${deleteIsCurrentPiProvider
                  ? t('connections.delete.dialogPiCurrent')
                  : deleteConnectionDialogDescription({
                      isCurrent: deleteIsCurrent,
                      agentName: agentDisplayName(deleteTicket.agentId),
                    }, t)}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          {deleteSwitchTargets.length > 0 ? (
            <div className="space-y-1.5">
              <p className="text-meta text-secondary">{t('connections.delete.switchFirst')}</p>
              <div className="flex flex-wrap gap-2">
                {deleteSwitchTargets.map((target) => (
                  <Button
                    key={target.id}
                    size="sm"
                    variant="outline"
                    disabled={deleteBusy || switchingTicketId != null}
                    onClick={() => void handleSwitchTicket(target)}
                  >
                    {t('connections.delete.switchTo', { label: target.label })}
                  </Button>
                ))}
              </div>
            </div>
          ) : null}
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
