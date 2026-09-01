import { useEffect, useMemo, useState } from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { TranslateFn } from '@/lib/i18n';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { getLocalEntryStatus } from '@/lib/api/adapter';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  isLocalEndpointKind,
  localEndpointBrandAgentId,
  localEndpointSurface,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from '@/pages/bridges/adapter-components';
import { localEndpointKindLabel } from '@/pages/bridges/route-pool-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/bridges/use-bridge-runtime-actions';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  BOARD_ROUTE_GRID,
  boardEndpointLoginTotals,
  buildBoardEndpointTypeRows,
  buildLocalEntryControl,
  type BoardEndpointTypeRow,
  type LocalEntryControl,
} from '@/pages/routes/board/board-view-model';
import {
  poolSurfaceToUsageSurface,
  rememberedBoardUsageFilters,
  usageSurfaceToPoolSurface,
} from '@/pages/routes/board/board-usage-model';
import { BoardUsageSection } from '@/pages/routes/board/board-usage-section';

function localEntryStatusLabel(control: LocalEntryControl, t: TranslateFn): string {
  if (control.stopping) return t('routes.board.entryStopping');
  if (control.starting) return t('routes.board.entryStarting');
  if (control.running) return t('routes.board.entryRunning');
  if (control.action === 'start') return t('routes.board.entryStopped');
  return t('routes.board.entryEmpty');
}

function rememberKind(raw: string): LocalEndpointKind | 'all' {
  if (raw === 'all' || !raw) return 'all';
  if (isLocalEndpointKind(raw)) return raw;
  const surface = usageSurfaceToPoolSurface(raw);
  if (surface === 'messages') return 'messages';
  if (surface === 'chat_completions') return 'chat_completions';
  if (surface === 'responses') return 'responses_codex';
  return 'all';
}

function BoardEndpointCard({
  row,
  selected,
  onSelect,
}: {
  row: BoardEndpointTypeRow;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t } = useI18n();
  const label = localEndpointKindLabel(row.kind, t);
  const logins = t('routes.board.endpointLogins', {
    oauth: row.oauthCount,
    apikey: row.apikeyCount,
  });
  const color = agentCssVar(localEndpointBrandAgentId(row.kind));

  return (
    <Card
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      aria-label={label}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        'cursor-pointer p-3 transition-colors hover:border-accent/40 hover:bg-hover/40',
        selected && 'border-accent bg-hover/40',
        row.oauthCount + row.apikeyCount === 0 && 'opacity-70',
      )}
    >
      <Tip className="block min-w-0 truncate font-mono text-sm font-medium" label={row.path}>
        <span style={{ color }}>{row.path}</span>
      </Tip>
      <p className="mt-1 text-xs text-secondary">{label}</p>
      <p className="mt-1 text-xs text-muted">{logins}</p>
    </Card>
  );
}

function BoardRouteSkeleton({ count }: { count: number }) {
  const n = Math.max(1, count);
  return (
    <div className={BOARD_ROUTE_GRID}>
      {Array.from({ length: n }).map((_, i) => (
        <Card key={i} className="p-3">
          <Skeleton className="h-4 w-28" />
          <Skeleton className="mt-2 h-3 w-36" />
        </Card>
      ))}
    </div>
  );
}

export default function RoutesBoardPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const {
    profiles,
    profileState,
    errors,
    loading,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    bridgeStatuses,
  } = useAdapterResources();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const [usageRefreshKey, setUsageRefreshKey] = useState(0);
  const [stopOpen, setStopOpen] = useState(false);
  const [gatewayRunning, setGatewayRunning] = useState(false);
  const { defaultPools, loading: poolsLoading } = useRoutePoolState({
    profiles,
    detailTarget: null,
    reloadKey: usageRefreshKey,
  });
  const {
    profileErrors,
    busyProfileIds,
    handleStartLocalEntry,
    handleStopLocalEntry,
  } = useBridgeRuntimeActions({
    profiles,
    hiddenTargetIds,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    t,
    toast,
  });
  const [endpointKind, setEndpointKind] = useState<LocalEndpointKind | 'all'>(() => (
    rememberKind(rememberedBoardUsageFilters().surface)
  ));
  useEffect(() => {
    void getLocalEntryStatus()
      .then((status) => {
        setGatewayRunning(status.running);
        for (const row of status.statuses) updateBridgeStatus(row);
      })
      .catch(() => undefined);
  }, [updateBridgeStatus, usageRefreshKey]);
  const localEntry = useMemo(
    () => buildLocalEntryControl(profiles, bridgeStatuses, hiddenTargetIds, defaultPools),
    [bridgeStatuses, defaultPools, hiddenTargetIds, profiles],
  );
  const localEntryBusy = localEntry.profileIds.some((id) => busyProfileIds[id])
    || Boolean(busyProfileIds.__local_entry__);
  const localEntryError = profileErrors.__local_entry__
    ?? localEntry.profileIds.map((id) => profileErrors[id]).find((error) => error != null)
    ?? null;

  const endpointRows = useMemo(
    () => buildBoardEndpointTypeRows(defaultPools, hiddenTargetIds),
    [defaultPools, hiddenTargetIds],
  );
  const totals = boardEndpointLoginTotals(endpointRows);
  const usageSurface = endpointKind === 'all'
    ? 'all'
    : poolSurfaceToUsageSurface(localEndpointSurface(endpointKind));
  const selectedRow = endpointKind === 'all'
    ? null
    : endpointRows.find((row) => row.kind === endpointKind) ?? null;
  const fleetLabel = totals.oauth + totals.apikey > 0
    ? t('routes.board.fleetLogins', { oauth: totals.oauth, apikey: totals.apikey })
    : t('routes.board.description');
  const loginHint = selectedRow
    ? t('routes.board.endpointLoginsHint', {
      oauth: selectedRow.oauthCount,
      apikey: selectedRow.apikeyCount,
    })
    : t('routes.board.endpointLoginsHintAll', {
      oauth: totals.oauth,
      apikey: totals.apikey,
    });
  const pageLoading = loading || poolsLoading;
  const showStatusSkeleton = pageLoading && defaultPools.length === 0;
  const entryRunning = localEntry.running || gatewayRunning;
  const entryLabel = localEntryStatusLabel(
    { ...localEntry, running: entryRunning, action: entryRunning ? 'stop' : localEntry.action },
    t,
  );
  const entryBadge = localEntry.profileIds.length === 0
    ? null
    : (
      <Badge variant={entryRunning ? 'success' : 'default'}>
        {entryLabel}
      </Badge>
    );

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.board.title')}
        badge={entryBadge}
        description={fleetLabel}
        descriptionTip={t('routes.board.descriptionTip')}
      />

      {profileState === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => {
            void reload();
            setUsageRefreshKey((key) => key + 1);
          }}
        />
      ) : (
        <div className={pageRhythm.blocks}>
          {localEntryError ? (
            <AdapterErrorLines
              error={localEntryError}
              fallback={localEntry.action === 'stop'
                ? t('routes.board.entryStopFailed')
                : t('routes.board.entryStartFailed')}
            />
          ) : null}
          <PageSection first aria-label={t('routes.board.statusSection')}>
            {showStatusSkeleton ? (
              <BoardRouteSkeleton count={4} />
            ) : (
              <div className={BOARD_ROUTE_GRID}>
                {endpointRows.map((row) => (
                  <BoardEndpointCard
                    key={row.kind}
                    row={row}
                    selected={endpointKind === row.kind}
                    onSelect={() => {
                      setEndpointKind((current) => (current === row.kind ? 'all' : row.kind));
                    }}
                  />
                ))}
              </div>
            )}
            <div className="mt-3 flex items-center gap-2">
              <p className="min-w-0 flex-1 text-sm text-secondary">{loginHint}</p>
              <PageRefreshButton
                className="ml-auto shrink-0"
                loading={pageLoading}
                onClick={() => {
                  void reload();
                  setUsageRefreshKey((key) => key + 1);
                }}
                label={t('routes.board.refresh')}
              />
              <Hint
                label={
                  localEntry.running || localEntry.action === 'start'
                    ? undefined
                    : entryLabel
                }
              >
                <Switch
                  checked={entryRunning}
                  disabled={localEntryBusy || localEntry.transitioning}
                  onCheckedChange={(on) => {
                    if (on) {
                      void handleStartLocalEntry().then((ok) => setGatewayRunning(ok));
                    } else {
                      setStopOpen(true);
                    }
                  }}
                  aria-label={t('routes.pool.entry')}
                />
              </Hint>
            </div>
          </PageSection>

          <BoardUsageSection
            profiles={profiles}
            hiddenTargetIds={hiddenTargetIds}
            pools={defaultPools}
            refreshKey={usageRefreshKey}
            surface={usageSurface}
          />
        </div>
      )}

      <Dialog
        open={stopOpen}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, localEntryBusy, () => setStopOpen(false))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={localEntryBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(localEntryBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(localEntryBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(localEntryBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.board.entryStopTitle')}</DialogTitle>
            <DialogDescription>{t('routes.board.entryStopDescription')}</DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button
              variant="secondary"
              onClick={() => setStopOpen(false)}
              disabled={localEntryBusy}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={localEntryBusy}
              onClick={() => {
                void handleStopLocalEntry().then((ok) => {
                  if (ok) {
                    setGatewayRunning(false);
                    setStopOpen(false);
                  }
                });
              }}
            >
              {localEntryBusy ? t('routes.stop.confirming') : t('routes.stop.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </RoutesPane>
  );
}
