import { useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Boxes, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { ListRow, ListRowBody, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
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
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { bridgesHrefForProfile, ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { fmtRelativeI18n } from '@/pages/backups/backup-format';
import { AdapterErrorLines } from '@/pages/bridges/adapter-components';
import { adapterBridgeUpstreamLabel } from '@/pages/bridges/adapter-labels';
import {
  adapterProfileFlowLabel,
  adapterProfilePrimaryAction,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
} from '@/pages/bridges/adapter-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/bridges/use-bridge-runtime-actions';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  activityHref,
  boardAttentionReasonLabel,
  boardFleetSummary,
  boardLifetimeSummaryLabel,
  boardRecentSummaryLabel,
  buildRouteBoardStatusRows,
  partitionBoardRows,
  type RouteBoardStatusRow,
} from '@/pages/routes/board/board-view-model';
import { BoardUsageSection } from '@/pages/routes/board/board-usage-section';

function BoardRouteRow({
  row,
  busy,
  targetHidden,
  onStart,
  onStop,
}: {
  row: RouteBoardStatusRow;
  busy: boolean;
  targetHidden: boolean;
  onStart: () => void;
  onStop: () => void;
}) {
  const { t } = useI18n();
  const view = bridgeRuntimeStatusView({
    route: 'local_bridge',
    bridgeState: row.state,
    statusUnavailable: row.statusUnavailable,
  }, t);
  const action = adapterProfilePrimaryAction({
    route: 'local_bridge',
    bridgeState: row.state,
    lastErrorCode: row.lastErrorCode,
    statusUnavailable: row.statusUnavailable,
  }, t);
  const transitioning = row.state === 'starting' || row.state === 'stopping';
  const relativeLast = row.recent.lastAt ? fmtRelativeI18n(row.recent.lastAt, t) : null;
  const recentLabel = boardRecentSummaryLabel(row.recent, relativeLast, t);
  const lifetimeLabel = boardLifetimeSummaryLabel(row.recent, t);
  const attentionLabel = boardAttentionReasonLabel(row.attentionReason, row.lastErrorCode, t);
  const uptimeLabel = row.startedAt && (row.state === 'running' || row.state === 'degraded')
    ? t('routes.board.uptime', { relative: fmtRelativeI18n(row.startedAt, t) })
    : t('routes.board.uptimeStopped');

  return (
    <ListRow className={LIST_ROW_PAD}>
      <ListRowBody
        leading={
          view ? (
            <StatusPin
              tone={view.tone}
              size="md"
              className={view.pulse ? 'animate-pulse' : undefined}
            />
          ) : undefined
        }
        main={
          <>
            <span className="min-w-0 truncate text-sm font-medium text-primary">{row.name}</span>
            {view ? (
              <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
            ) : null}
            <span className="font-mono text-meta text-muted">
              {row.endpoint ?? t('routes.pendingPort')}
            </span>
            <span className="text-meta text-muted">
              {adapterBridgeUpstreamLabel(row.upstreamStatus, t)}
            </span>
            <span className="text-meta text-muted">{uptimeLabel}</span>
            {lifetimeLabel ? <span className="text-meta text-muted">{lifetimeLabel}</span> : null}
            {recentLabel ? <span className="text-meta text-muted">{recentLabel}</span> : null}
            {attentionLabel ? (
              <span className="w-full text-meta text-warning">{attentionLabel}</span>
            ) : null}
          </>
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            {action ? (
              <Hint
                label={
                  targetHidden && action.kind !== 'stop'
                    ? t('routes.targetHiddenHint')
                    : undefined
                }
              >
                <Button
                  variant={action.kind === 'stop' ? 'dangerOutline' : 'outline'}
                  size="sm"
                  disabled={busy || transitioning || (targetHidden && action.kind !== 'stop')}
                  onClick={() => (action.kind === 'stop' ? onStop() : onStart())}
                >
                  {busy ? t('routes.busy') : action.label}
                </Button>
              </Hint>
            ) : null}
            <Link
              to={activityHref({ route: row.profileId })}
              className="text-meta text-secondary hover:text-primary"
            >
              {t('routes.board.viewRequests')}
            </Link>
            <Link
              to={bridgesHrefForProfile(row.profileId)}
              className="text-meta text-secondary hover:text-primary"
            >
              {t('routes.detail')}
            </Link>
          </div>
        }
      />
    </ListRow>
  );
}

export default function RoutesBoardPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { toast } = useToast();
  const {
    profiles,
    bridgeStatuses,
    entries,
    profileState,
    errors,
    loading,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
  } = useAdapterResources();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);

  const runtime = useBridgeRuntimeActions({
    profiles,
    hiddenTargetIds,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    t,
    toast,
  });
  const {
    stopConfirm,
    setStopConfirm,
    profileErrors,
    busyProfileIds,
    handleStartBridge,
    confirmStopBridge,
  } = runtime;

  const statusRows = useMemo(
    () => buildRouteBoardStatusRows(profiles, bridgeStatuses, errors.bridgeStatuses),
    [profiles, bridgeStatuses, errors.bridgeStatuses],
  );
  const { attention, rest } = useMemo(() => partitionBoardRows(statusRows), [statusRows]);
  const [usageRefreshKey, setUsageRefreshKey] = useState(0);
  const fleet = boardFleetSummary(statusRows, t);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.board.title')}
        description={t('routes.board.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">
          {fleet?.label ?? t('routes.board.noFleet')}
        </p>
        <div className={pageRhythm.chromeActions}>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={loading}
            onClick={() => {
              void reload();
              setUsageRefreshKey((key) => key + 1);
            }}
          >
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
            {t('routes.board.refresh')}
          </Button>
        </div>
      </div>

      {profileState === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => {
            void reload();
            setUsageRefreshKey((key) => key + 1);
          }}
        />
      ) : statusRows.length === 0 && !loading ? (
        <EmptyState
          icon={Boxes}
          title={t('routes.board.emptyTitle')}
          description={t('routes.board.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(ROUTES_POOL_PATH)}
            >
              {t('routes.nav.goToList')}
            </Button>
          }
        />
      ) : (
        <div className={pageRhythm.blocks}>
          <BoardUsageSection
            profiles={profiles}
            hiddenTargetIds={hiddenTargetIds}
            refreshKey={usageRefreshKey}
          />

          {attention.length > 0 ? (
            <PageSection title={t('routes.board.attentionSection')}>
              <div className={pageRhythm.stackDense}>
                {attention.map((row) => (
                  <BoardRouteRow
                    key={row.profileId}
                    row={row}
                    busy={busyProfileIds[row.profileId] === true}
                    targetHidden={hiddenTargetIds.has(row.profile.targetAgentId)}
                    onStart={() => void handleStartBridge(row.profile)}
                    onStop={() => setStopConfirm(row.profile)}
                  />
                ))}
              </div>
            </PageSection>
          ) : null}

          {rest.length > 0 ? (
            <PageSection
              title={attention.length > 0 ? t('routes.board.otherSection') : t('routes.board.statusSection')}
            >
              <div className={pageRhythm.stackDense} aria-label={t('routes.board.statusSection')}>
                {rest.map((row) => (
                  <BoardRouteRow
                    key={row.profileId}
                    row={row}
                    busy={busyProfileIds[row.profileId] === true}
                    targetHidden={hiddenTargetIds.has(row.profile.targetAgentId)}
                    onStart={() => void handleStartBridge(row.profile)}
                    onStop={() => setStopConfirm(row.profile)}
                  />
                ))}
              </div>
            </PageSection>
          ) : null}
        </div>
      )}

      <Dialog
        open={Boolean(stopConfirm)}
        onOpenChange={(open) =>
          closeConfirmationOnOpenChange(open, stopDialogBusy, () => setStopConfirm(null))
        }
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={stopDialogBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(stopDialogBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.stop.title')}</DialogTitle>
            <DialogDescription>{t('routes.stop.description')}</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm ? (
              <p className="text-sm text-secondary">
                {adapterProfileFlowLabel(stopConfirm, entries)}
              </p>
            ) : null}
            {stopError ? (
              <AdapterErrorLines error={stopError} fallback={t('routes.stop.fallback')} />
            ) : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button
              variant="secondary"
              onClick={() => setStopConfirm(null)}
              disabled={stopDialogBusy}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => void confirmStopBridge()}
              disabled={stopDialogBusy}
            >
              {stopDialogBusy ? t('routes.stop.confirming') : t('routes.stop.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </RoutesPane>
  );
}
