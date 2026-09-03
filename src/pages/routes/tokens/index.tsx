import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound, Plus, Sparkles } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { deleteAccount, listAccounts } from '@/lib/api/account';
import { deleteProvider, listProviders } from '@/lib/api/provider';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { localEndpointKindFromPool } from '@/lib/route-endpoints';
import { ROUTES_POOL_PATH } from '@/lib/routes-path';
import {
  createLocalToken,
  deleteLocalToken,
  listLocalTokens,
  setLocalToken,
  setLocalTokenName,
} from '@/lib/api/adapter';
import type { LocalTokenRecord } from '@/lib/backend/contracts/adapter';
import { USAGE_COLLECTED_EVENT } from '@/lib/usage-sync';
import { ROUTES_INSPECT_WIDTH_KEY } from '@/pages/routes/shared/route-inspect';
import { useAdapterResources } from '@/pages/routes/shared/use-bridge-resources';
import { useRoutePoolState } from '@/pages/routes/shared/use-route-pool-state';
import { boardUsageWindow } from '@/pages/routes/board/board-usage-model';
import { useBoardUsageStats } from '@/pages/routes/board/use-board-usage';
import { buildLocalGatewayControl } from '@/pages/routes/board/board-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { CreateTokenEndpointCards } from './CreateTokenEndpointCards';
import { TokenDetailPanel } from './TokenDetailPanel';
import { TokenList } from './TokenList';
import {
  attachTokenUsage,
  buildCreateTokenEndpointCards,
  buildLocalTokenRows,
  defaultCreateTokenName,
  firstCreateTokenPoolId,
  generateLocalToken,
  localTokenDeleteGate,
  localTokenEditKeyGate,
  maskLocalToken,
  tokenDisplayName,
  tokenTypeLabel,
  type LocalTokenRow,
} from './tokens-model';
import { TokenImportToAgentButton } from './TokenImportToAgentButton';
import {
  connectionMatchAgentNames,
  hashLocalToken,
  matchesConnectionEntryKeys,
  type ConnectionEntryKeyMatch,
} from './token-connection-matches';
import type { TokenImportAgentRef } from './token-import-model';

export default function RoutesTokensPage() {
  const { t, lang } = useI18n();
  const navigate = useNavigate();
  const { toast } = useToast();
  const { hiddenIds, installedAgents } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const installedAgentRefs = useMemo<TokenImportAgentRef[]>(
    () => installedAgents.map((agent) => ({ id: agent.id, name: agent.name })),
    [installedAgents],
  );
  const {
    profiles,
    bridgeStatuses,
    errors,
    profileState,
    loading,
    reload,
  } = useAdapterResources();
  const [tokenTick, setTokenTick] = useState(0);
  const [collectKey, setCollectKey] = useState(0);
  const [tokenRecords, setTokenRecords] = useState<LocalTokenRecord[] | null>(null);
  const [editRow, setEditRow] = useState<LocalTokenRow | null>(null);
  const [editValue, setEditValue] = useState('');
  const [editBusy, setEditBusy] = useState(false);
  const [importAfterSaveRow, setImportAfterSaveRow] = useState<LocalTokenRow | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState('');
  const [createPoolId, setCreatePoolId] = useState('');
  const [createBusy, setCreateBusy] = useState(false);
  const [deleteRow, setDeleteRow] = useState<LocalTokenRow | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [connectionMatches, setConnectionMatches] = useState<ConnectionEntryKeyMatch[]>([]);
  const [connectionMatchesReady, setConnectionMatchesReady] = useState(true);
  const [alsoDeleteConnections, setAlsoDeleteConnections] = useState(true);
  const {
    chatCompletionsShared,
    defaultPools,
    loading: poolsLoading,
  } = useRoutePoolState({
    profiles,
    detailTarget: null,
    reloadKey: tokenTick,
  });
  const inspect = useSideSplit<string>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const localGateway = useMemo(
    () => buildLocalGatewayControl(profiles, bridgeStatuses, hiddenTargetIds, defaultPools),
    [bridgeStatuses, defaultPools, hiddenTargetIds, profiles],
  );

  useEffect(() => {
    let cancelled = false;
    void listLocalTokens()
      .then((records) => {
        if (cancelled) return;
        setTokenRecords(records);
      })
      .catch(() => {
        if (!cancelled) setTokenRecords([]);
      });
    return () => {
      cancelled = true;
    };
  }, [defaultPools, tokenTick]);

  const rows = useMemo(
    () => buildLocalTokenRows(
      profiles,
      bridgeStatuses,
      errors.bridgeStatuses,
      defaultPools,
      chatCompletionsShared,
      Object.fromEntries((tokenRecords ?? []).filter((record) => record.primary).map((record) => [record.poolId, record.token])),
      tokenRecords,
    ),
    [
      bridgeStatuses,
      chatCompletionsShared,
      defaultPools,
      errors.bridgeStatuses,
      profiles,
      tokenRecords,
    ],
  );
  const usageWindow = useMemo(() => boardUsageWindow('7d'), []);
  const usageState = useBoardUsageStats({
    enabled: rows.length > 0,
    since: usageWindow.since,
    refreshKey: tokenTick + collectKey,
  });
  const listRows = useMemo(
    () => (usageState.status === 'ready' ? attachTokenUsage(rows, usageState.rows) : rows),
    [rows, usageState],
  );
  const createTargets = useMemo(
    () => defaultPools.flatMap((pool) => {
      if (pool.members.length === 0) return [];
      const kind = localEndpointKindFromPool(pool);
      return kind ? [{ id: pool.id, kind }] : [];
    }),
    [defaultPools],
  );
  const createCards = useMemo(
    () => buildCreateTokenEndpointCards(createTargets),
    [createTargets],
  );

  useEffect(() => {
    const onCollected = () => setCollectKey((key) => key + 1);
    window.addEventListener(USAGE_COLLECTED_EVENT, onCollected);
    return () => window.removeEventListener(USAGE_COLLECTED_EVENT, onCollected);
  }, []);

  const openEdit = (row: LocalTokenRow) => {
    const gate = localTokenEditKeyGate(row, t);
    if (!gate.enabled) {
      toast({
        title: gate.reason ?? t('routes.tokens.editKeyNeedPool'),
        variant: 'danger',
      });
      return;
    }
    setEditRow(row);
    setEditValue(row.token ?? '');
  };

  const saveEdit = async () => {
    if (!editRow || editBusy) return;
    // setLocalToken requires a real pool id — never call it with a leftover profile id.
    if (!editRow.poolBacked) {
      toast({ title: t('routes.tokens.editKeyNeedPool'), variant: 'danger' });
      setEditRow(null);
      return;
    }
    const token = editValue.trim();
    if (!token) {
      toast({ title: t('routes.tokens.keyRequired'), variant: 'danger' });
      return;
    }
    setEditBusy(true);
    try {
      await setLocalToken(editRow.id, token);
      setEditRow(null);
      setImportAfterSaveRow({
        ...editRow,
        token,
        maskedToken: maskLocalToken(token),
      });
      setTokenTick((tick) => tick + 1);
      void reload();
    } catch {
      toast({ title: t('routes.tokens.editKeyFailed'), variant: 'danger' });
    } finally {
      setEditBusy(false);
    }
  };

  const openCreate = () => {
    if (createTargets.length === 0) {
      toast({ title: t('routes.tokens.createNeedPool'), variant: 'danger' });
      return;
    }
    setCreatePoolId(firstCreateTokenPoolId(createCards));
    setCreateName('');
    setCreateOpen(true);
  };

  const saveCreate = async () => {
    if (createBusy) return;
    if (!createPoolId) {
      toast({ title: t('routes.tokens.createNeedPool'), variant: 'danger' });
      return;
    }
    const typedName = createName.trim();
    const kind = createCards.find((card) => card.poolId === createPoolId)?.kind
      ?? createTargets.find((row) => row.id === createPoolId)?.kind;
    const name = typedName || (kind
      ? defaultCreateTokenName({
        kind,
        existingNames: listRows.filter((row) => row.kind === kind).map((row) => row.name),
        t,
      })
      : '');
    if (!name) {
      toast({ title: t('routes.tokens.nameRequired'), variant: 'danger' });
      return;
    }
    setCreateBusy(true);
    try {
      const created = await createLocalToken(createPoolId, name);
      setCreateOpen(false);
      setTokenTick((tick) => tick + 1);
      inspect.open(created.id);
      void reload();
    } catch {
      toast({ title: t('routes.tokens.createFailed'), variant: 'danger' });
    } finally {
      setCreateBusy(false);
    }
  };

  const saveName = async (row: LocalTokenRow, name: string) => {
    const trimmed = name.trim();
    if (!trimmed) {
      toast({ title: t('routes.tokens.nameRequired'), variant: 'danger' });
      return;
    }
    try {
      await setLocalTokenName(row.id, trimmed);
      setTokenTick((tick) => tick + 1);
    } catch {
      toast({ title: t('routes.tokens.saveNameFailed'), variant: 'danger' });
    }
  };

  useEffect(() => {
    if (!deleteRow) {
      setConnectionMatches([]);
      setAlsoDeleteConnections(true);
      setConnectionMatchesReady(true);
      return;
    }
    const token = deleteRow.token?.trim() ?? '';
    if (!token) {
      setConnectionMatches([]);
      setConnectionMatchesReady(true);
      return;
    }
    let cancelled = false;
    setConnectionMatchesReady(false);
    void Promise.all([listProviders(), listAccounts(), hashLocalToken(token)])
      .then(([providers, accounts, tokenHash]) => {
        if (cancelled) return;
        const matches = matchesConnectionEntryKeys({ tokenHash, providers, accounts });
        setConnectionMatches(matches);
        setAlsoDeleteConnections(matches.length > 0);
        setConnectionMatchesReady(true);
      })
      .catch(() => {
        if (cancelled) return;
        setConnectionMatches([]);
        setConnectionMatchesReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [deleteRow]);

  const confirmDelete = async () => {
    if (!deleteRow || deleteBusy) return;
    if (deleteRow.token?.trim() && !connectionMatchesReady) return;
    const gate = localTokenDeleteGate(deleteRow, t);
    if (!gate.enabled) {
      toast({ title: gate.reason ?? t('routes.tokens.deleteFailed'), variant: 'danger' });
      setDeleteRow(null);
      return;
    }
    const removeConnections = alsoDeleteConnections && connectionMatches.length > 0;
    const matches = connectionMatches;
    setDeleteBusy(true);
    try {
      await deleteLocalToken(deleteRow.id);
      if (removeConnections) {
        try {
          for (const match of matches) {
            if (match.sourceKind === 'account') {
              await deleteAccount(match.agentId, match.sourceId);
            } else {
              await deleteProvider(match.agentId, match.sourceId);
            }
          }
        } catch {
          toast({ title: t('routes.tokens.deleteAlsoConnectionsFailed'), variant: 'danger' });
        }
      }
      if (inspect.target === deleteRow.id) inspect.close();
      setDeleteRow(null);
      setTokenTick((tick) => tick + 1);
      void reload();
    } catch {
      toast({ title: t('routes.tokens.deleteFailed'), variant: 'danger' });
    } finally {
      setDeleteBusy(false);
    }
  };

  const detailRow = inspect.target
    ? listRows.find((row) => row.id === inspect.target) ?? null
    : null;
  const pageLoading = loading || poolsLoading;

  const list = (
    <RoutesPane>
      <PageHeader
        title={t('routes.tokens.title')}
        description={t('routes.tokens.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">{
          !localGateway.running
            && localGateway.profileIds.length === 0
            && localGateway.hasEnrolledLogins
            ? t('routes.board.entryNeedRoute')
            : t('routes.tokens.scopeNote')
        }</p>
        <div className={pageRhythm.chromeActions}>
          <Button
            size="sm"
            variant="outline"
            disabled={pageLoading || createTargets.length === 0}
            onClick={openCreate}
            title={createTargets.length === 0 ? t('routes.tokens.createNeedPool') : undefined}
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
            {t('routes.tokens.create')}
          </Button>
          <PageRefreshButton
            loading={pageLoading}
            onClick={() => {
              setTokenTick((tick) => tick + 1);
              void reload();
            }}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

      {profileState === 'error' && listRows.length === 0 && !pageLoading ? (
        <ErrorState
          title={t('routes.runtime.unavailable')}
          error={errors.profiles ?? new Error(t('routes.runtime.unavailable'))}
          onRetry={() => void reload()}
        />
      ) : listRows.length === 0 && !pageLoading ? (
        <EmptyState
          icon={KeyRound}
          title={t('routes.tokens.emptyTitle')}
          description={t('routes.tokens.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(ROUTES_POOL_PATH)}
            >
              {t('routes.nav.goToPool')}
            </Button>
          }
        />
      ) : (
        <PageSection first>
          <TokenList
            rows={listRows}
            activeId={inspect.target}
            onShowDetail={(row) => inspect.open(row.id)}
            onDelete={(row) => setDeleteRow(row)}
            installedAgents={installedAgentRefs}
          />
        </PageSection>
      )}

      <Dialog open={editRow != null} onOpenChange={(open) => { if (!open && !editBusy) setEditRow(null); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.editKeyTitle')}</DialogTitle>
            <DialogDescription>
              {editRow ? tokenTypeLabel(editRow, t) : null}
            </DialogDescription>
          </DialogHeader>
          <div className="flex items-center gap-2">
            <Input
              className="min-w-0 flex-1"
              value={editValue}
              onChange={(event) => setEditValue(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              disabled={editBusy}
              aria-label={t('routes.tokens.fieldToken')}
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              disabled={editBusy}
              title={t('routes.tokens.generateKey')}
              aria-label={t('routes.tokens.generateKey')}
              onClick={() => setEditValue(generateLocalToken())}
            >
              <Sparkles className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </div>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setEditRow(null)} disabled={editBusy}>
              {t('common.cancel')}
            </Button>
            <Button onClick={() => { void saveEdit(); }} disabled={editBusy}>
              {t('routes.tokens.saveKey')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={importAfterSaveRow != null}
        onOpenChange={(open) => { if (!open) setImportAfterSaveRow(null); }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.importAfterSaveTitle')}</DialogTitle>
            <DialogDescription>
              {t('routes.tokens.importAfterSaveDescription')}
            </DialogDescription>
          </DialogHeader>
          {importAfterSaveRow ? (
            <div className="flex flex-wrap items-center gap-2">
              <TokenImportToAgentButton
                row={importAfterSaveRow}
                installedAgents={installedAgentRefs}
              />
            </div>
          ) : null}
          <DialogFooter>
            <Button variant="secondary" onClick={() => setImportAfterSaveRow(null)}>
              {t('routes.tokens.importAfterSaveSkip')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={createOpen} onOpenChange={(open) => { if (!open && !createBusy) setCreateOpen(false); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.createTitle')}</DialogTitle>
            <DialogDescription>{t('routes.tokens.createDescription')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <CreateTokenEndpointCards
              cards={createCards}
              value={createPoolId}
              onChange={setCreatePoolId}
              disabled={createBusy}
              unavailableReason={t('routes.tokens.createNeedPool')}
            />
            <div className="space-y-1">
              <p className="text-meta text-muted">{t('routes.tokens.fieldName')}</p>
              <Input
                value={createName}
                onChange={(event) => setCreateName(event.target.value)}
                placeholder={t('routes.tokens.createNamePlaceholder')}
                disabled={createBusy}
                aria-label={t('routes.tokens.fieldName')}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setCreateOpen(false)} disabled={createBusy}>
              {t('common.cancel')}
            </Button>
            <Button onClick={() => { void saveCreate(); }} disabled={createBusy}>
              {t('routes.tokens.create')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteRow != null} onOpenChange={(open) => { if (!open && !deleteBusy) setDeleteRow(null); }}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.tokens.deleteTitle')}</DialogTitle>
            <DialogDescription>
              {deleteRow
                ? `${tokenDisplayName(deleteRow, t)} · ${t('routes.tokens.deleteDescription')}`
                : t('routes.tokens.deleteDescription')}
            </DialogDescription>
          </DialogHeader>
          {connectionMatches.length > 0 ? (
            <label className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={alsoDeleteConnections}
                disabled={deleteBusy}
                onChange={(event) => setAlsoDeleteConnections(event.target.checked)}
              />
              <span className="min-w-0">
                <span className="block">{t('routes.tokens.deleteAlsoConnections')}</span>
                <span className="block text-meta text-muted">
                  {t('routes.tokens.deleteAlsoConnectionsHint', {
                    count: connectionMatches.length,
                    names: connectionMatchAgentNames(connectionMatches, agentDisplayName).join(lang === 'zh' ? '、' : ', '),
                  })}
                </span>
              </span>
            </label>
          ) : null}
          <DialogFooter>
            <Button variant="secondary" disabled={deleteBusy} onClick={() => setDeleteRow(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" disabled={deleteBusy || Boolean(deleteRow?.token?.trim() && !connectionMatchesReady)} onClick={() => { void confirmDelete(); }}>
              {t('routes.tokens.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </RoutesPane>
  );

  return (
    <WorkbenchSplitPage
      split={inspect}
      resizeAria={t('common.resizeSidePanel')}
      panel={detailRow ? (
        <TokenDetailPanel
          row={detailRow}
          width={inspect.paneWidth}
          onClose={() => inspect.close()}
          onEditKey={() => openEdit(detailRow)}
          onSaveName={detailRow.poolBacked ? (name) => saveName(detailRow, name) : undefined}
          onDelete={() => setDeleteRow(detailRow)}
          installedAgents={installedAgentRefs}
        />
      ) : null}
    >
      {list}
    </WorkbenchSplitPage>
  );
}
