import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Boxes, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { bridgesHrefForProfile, ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from '@/pages/bridges/adapter-components';
import {
  adapterProfileFlowLabel,
  adapterProfilePrimaryAction,
  bridgeRuntimeStatusView,
} from '@/pages/bridges/adapter-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/bridges/use-bridge-runtime-actions';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  BOARD_ROUTE_GRID,
  boardFleetSummary,
  buildRouteBoardStatusRows,
  type RouteBoardStatusRow,
} from '@/pages/routes/board/board-view-model';
import { BoardUsageSection } from '@/pages/routes/board/board-usage-section';

function BoardRouteCard({
  row,
  busy,
  onStart,
  onStop,
}: {
  row: RouteBoardStatusRow;
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
}) {
  const { t } = useI18n();
  const navigate = useNavigate();
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
  const go = () => navigate(bridgesHrefForProfile(row.profileId));

  return (
    <Card
      role="button"
      tabIndex={0}
      aria-label={row.name}
      onClick={go}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          go();
        }
      }}
      className={cn(
        'cursor-pointer p-3 transition-colors hover:border-accent/40 hover:bg-hover/40',
        row.needsAttention && 'border-warning/40',
      )}
    >
      <div className="flex items-center gap-2">
        <AgentLogo agentId={row.profile.targetAgentId} size="sm" />
        <Tip className="min-w-0 flex-1 truncate text-sm font-medium" label={row.name}>
          {row.name}
        </Tip>
        {view ? (
          <Tip className="shrink-0" label={view.label}>
            <StatusPin
              tone={view.tone}
              size="md"
              className={view.pulse ? 'animate-pulse' : undefined}
            />
          </Tip>
        ) : null}
      </div>
      <div className="mt-1.5 flex min-w-0 items-center gap-1">
        <Tip
          className="min-w-0 truncate font-mono text-xs text-muted"
          label={row.endpoint ?? t('routes.pendingPort')}
        >
          {row.endpoint ?? t('routes.pendingPort')}
        </Tip>
        {view ? (
          <Badge
            variant={
              view.tone === 'success'
                ? 'success'
                : view.tone === 'warning'
                  ? 'warning'
                  : view.tone === 'danger'
                    ? 'danger'
                    : 'default'
            }
            className="h-5 shrink-0 px-1.5 text-meta"
          >
            {view.label}
          </Badge>
        ) : null}
        {action ? (
          <Button
            className="ml-auto shrink-0"
            variant={action.kind === 'stop' ? 'dangerOutline' : 'outline'}
            size="sm"
            disabled={busy || transitioning}
            onClick={(event) => {
              event.stopPropagation();
              if (action.kind === 'stop') onStop();
              else onStart();
            }}
          >
            {busy ? t('routes.busy') : action.label}
          </Button>
        ) : null}
      </div>
    </Card>
  );
}

function BoardRouteSkeleton({ count }: { count: number }) {
  const n = Math.max(1, count);
  return (
    <div className={BOARD_ROUTE_GRID}>
      {Array.from({ length: n }).map((_, i) => (
        <Card key={i} className="p-3">
          <div className="flex items-center gap-2">
            <Skeleton className="h-6 w-6 shrink-0 rounded-full" />
            <Skeleton className="h-4 w-24" />
          </div>
          <Skeleton className="mt-2 h-4 w-32" />
        </Card>
      ))}
    </div>
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
    () => buildRouteBoardStatusRows(
      profiles,
      bridgeStatuses,
      errors.bridgeStatuses,
      hiddenTargetIds,
    ),
    [profiles, bridgeStatuses, errors.bridgeStatuses, hiddenTargetIds],
  );
  const [usageRefreshKey, setUsageRefreshKey] = useState(0);
  const fleet = boardFleetSummary(statusRows, t);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const showStatusSkeleton = loading && statusRows.length === 0;

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.board.title')}
        description={fleet?.label ?? t('routes.board.description')}
        descriptionTip={t('routes.board.descriptionTip')}
      />
      <div className={pageRhythm.chromeRow}>
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
          <PageSection first aria-label={t('routes.board.statusSection')}>
            {showStatusSkeleton ? (
              <BoardRouteSkeleton count={4} />
            ) : (
              <div className={BOARD_ROUTE_GRID}>
                {statusRows.map((row) => (
                  <BoardRouteCard
                    key={row.profileId}
                    row={row}
                    busy={busyProfileIds[row.profileId] === true}
                    onStart={() => void handleStartBridge(row.profile)}
                    onStop={() => setStopConfirm(row.profile)}
                  />
                ))}
              </div>
            )}
          </PageSection>

          <BoardUsageSection
            profiles={profiles}
            hiddenTargetIds={hiddenTargetIds}
            refreshKey={usageRefreshKey}
          />
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
