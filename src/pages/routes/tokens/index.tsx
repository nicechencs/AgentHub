import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound } from 'lucide-react';
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
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { listLocalTokens, setLocalToken } from '@/lib/api/adapter';
import { ROUTES_INSPECT_WIDTH_KEY } from '@/pages/bridges/route-inspect';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { buildLocalEntryControl } from '@/pages/routes/board/board-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { TokenDetailPanel } from './TokenDetailPanel';
import { TokenList } from './TokenList';
import { buildLocalTokenRows, tokenTypeLabel, type LocalTokenRow } from './tokens-model';

export default function RoutesTokensPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { toast } = useToast();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const {
    profiles,
    bridgeStatuses,
    errors,
    profileState,
    loading,
    reload,
  } = useAdapterResources();
  const [tokenTick, setTokenTick] = useState(0);
  const [tokensByPoolId, setTokensByPoolId] = useState<Record<string, string>>({});
  const [editRow, setEditRow] = useState<LocalTokenRow | null>(null);
  const [editValue, setEditValue] = useState('');
  const [editBusy, setEditBusy] = useState(false);
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
  const localEntry = useMemo(
    () => buildLocalEntryControl(profiles, bridgeStatuses, hiddenTargetIds, defaultPools),
    [bridgeStatuses, defaultPools, hiddenTargetIds, profiles],
  );

  useEffect(() => {
    let cancelled = false;
    void listLocalTokens()
      .then((records) => {
        if (cancelled) return;
        const next: Record<string, string> = {};
        for (const record of records) next[record.poolId] = record.token;
        setTokensByPoolId(next);
      })
      .catch(() => {
        if (!cancelled) setTokensByPoolId({});
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
      tokensByPoolId,
    ),
    [
      bridgeStatuses,
      chatCompletionsShared,
      defaultPools,
      errors.bridgeStatuses,
      profiles,
      tokensByPoolId,
    ],
  );

  const openEdit = (row: LocalTokenRow) => {
    setEditRow(row);
    setEditValue(row.token ?? '');
  };

  const saveEdit = async () => {
    if (!editRow || editBusy) return;
    const token = editValue.trim();
    if (!token) {
      toast({ title: t('routes.tokens.keyRequired'), variant: 'danger' });
      return;
    }
    setEditBusy(true);
    try {
      await setLocalToken(editRow.id, token);
      setEditRow(null);
      setTokenTick((tick) => tick + 1);
      void reload();
    } catch {
      toast({ title: t('routes.tokens.editKeyFailed'), variant: 'danger' });
    } finally {
      setEditBusy(false);
    }
  };

  const detailRow = inspect.target
    ? rows.find((row) => row.id === inspect.target) ?? null
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
          !localEntry.running
            && localEntry.profileIds.length === 0
            && localEntry.hasEnrolledLogins
            ? t('routes.board.entryNeedRoute')
            : t('routes.tokens.scopeNote')
        }</p>
        <div className={pageRhythm.chromeActions}>
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

      {profileState === 'error' && rows.length === 0 && !pageLoading ? (
        <ErrorState
          title={t('routes.runtime.unavailable')}
          error={errors.profiles ?? new Error(t('routes.runtime.unavailable'))}
          onRetry={() => void reload()}
        />
      ) : rows.length === 0 && !pageLoading ? (
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
            rows={rows}
            activeId={inspect.target}
            onShowDetail={(row) => inspect.open(row.id)}
            onEditKey={openEdit}
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
          <Input
            value={editValue}
            onChange={(event) => setEditValue(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            disabled={editBusy}
            aria-label={t('routes.tokens.fieldToken')}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditRow(null)} disabled={editBusy}>
              {t('common.cancel')}
            </Button>
            <Button onClick={() => { void saveEdit(); }} disabled={editBusy}>
              {t('routes.tokens.saveKey')}
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
        />
      ) : null}
    >
      {list}
    </WorkbenchSplitPage>
  );
}
