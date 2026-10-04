import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentLogo } from '@/components/shared/AgentLogo';
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
import { agentDisplayName } from '@/config/agents';
import { getLocalGatewayStatus, listLocalTokens } from '@/lib/api/adapter';
import {
  getGoRouteIsolatedStatus,
  shouldApplyGoRouteResult,
  startGoRouteIsolated,
  stopGoRouteIsolated,
  type GoRouteIsolatedState,
  type GoRouteIsolatedStatus,
} from '@/lib/api/go-route-isolated';
import type { LocalTokenRecord } from '@/lib/backend/contracts/adapter';
import { BackendUnavailableError } from '@/lib/backend/contracts/errors';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  isLocalEndpointKind,
  localEndpointBrandAgentId,
  localEndpointSurface,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { agentCssVar } from '@/styles/tokens';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from '@/pages/routes/shared/adapter-components';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';
import { useAdapterResources } from '@/pages/routes/shared/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/routes/shared/use-bridge-runtime-actions';
import { useRoutePoolState } from '@/pages/routes/shared/use-route-pool-state';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  BOARD_ROUTE_GRID,
  boardEndpointKeyTotals,
  buildBoardEndpointTypeRows,
  buildLocalGatewayControl,
  localGatewayRetryAction as resolveLocalGatewayRetryAction,
  type BoardEndpointTypeRow,
  type LocalGatewayControl,
  visibleBoardEndpointKind,
} from '@/pages/routes/board/board-view-model';
import {
  buildLocalTokenRows,
  supportedAgentsForEndpointKind,
  visibleTokenKinds,
} from '@/pages/routes/tokens/tokens-model';
import {
  poolSurfaceToUsageSurface,
  rememberedBoardUsageFilters,
  usageSurfaceToPoolSurface,
} from '@/pages/routes/board/board-usage-model';
import { BoardUsageSection } from '@/pages/routes/board/board-usage-section';

function localGatewayStatusLabel(control: LocalGatewayControl, t: TranslateFn): string {
  if (control.restarting) return t('routes.localForward.restarting');
  if (control.stopping) return t('routes.board.entryStopping');
  if (control.starting) return t('routes.board.entryStarting');
  if (control.running) return t('routes.board.entryRunning');
  if (control.action === 'start') return t('routes.board.entryStopped');
  return t('routes.board.entryEmpty');
}

const GO_ISOLATED_STOPPED: GoRouteIsolatedStatus = {
  state: 'stopped',
  listenReady: false,
  port: null,
  lastError: null,
  home: null,
  lifecycle: 'stopped',
  inFlightCount: 0,
  memberCount: 0,
  healthyMemberCount: 0,
  recovering: false,
  restartCount: 0,
};

function goIsolatedStatusLabel(state: GoRouteIsolatedState, t: TranslateFn): string {
  if (state === 'starting') return t('routes.board.goIsolatedStarting');
  if (state === 'ready') return t('routes.board.goIsolatedReady');
  if (state === 'failed') return t('routes.board.goIsolatedFailed');
  return t('routes.board.goIsolatedStopped');
}

function goIsolatedBadgeVariant(
  state: GoRouteIsolatedState,
): 'success' | 'warning' | 'danger' | 'default' {
  if (state === 'ready') return 'success';
  if (state === 'starting') return 'warning';
  if (state === 'failed') return 'danger';
  return 'default';
}

function failedGoIsolatedStatus(current: GoRouteIsolatedStatus): GoRouteIsolatedStatus {
  return {
    ...current,
    state: 'failed',
    listenReady: false,
    port: null,
    lastError: null,
    lifecycle: 'unavailable',
    inFlightCount: 0,
    memberCount: 0,
    healthyMemberCount: 0,
    recovering: false,
  };
}

function GoIsolatedStrip() {
  const { t } = useI18n();
  const [status, setStatus] = useState<GoRouteIsolatedStatus>(GO_ISOLATED_STOPPED);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const generationRef = useRef(0);

  useEffect(() => {
    let cancelled = false;
    let refreshing = false;
    const refresh = () => {
      if (refreshing || busyRef.current) return;
      refreshing = true;
      const requestGeneration = generationRef.current;
      void getGoRouteIsolatedStatus()
        .then((next) => {
          if (shouldApplyGoRouteResult(
            requestGeneration,
            generationRef.current,
            cancelled || busyRef.current,
          )) {
            setStatus(next);
          }
        })
        .catch((error: unknown) => {
          if (
            !shouldApplyGoRouteResult(
              requestGeneration,
              generationRef.current,
              cancelled || busyRef.current,
            )
          ) {
            return;
          }
          if (error instanceof BackendUnavailableError) return;
          setStatus(failedGoIsolatedStatus);
        })
        .finally(() => {
          refreshing = false;
        });
    };
    refresh();
    const timer = window.setInterval(refresh, 1_000);
    return () => {
      cancelled = true;
      generationRef.current += 1;
      window.clearInterval(timer);
    };
  }, []);

  const run = useCallback(async (action: 'start' | 'stop') => {
    if (busyRef.current || status.state === 'starting') return;
    const actionGeneration = generationRef.current + 1;
    generationRef.current = actionGeneration;
    busyRef.current = true;
    setBusy(true);
    if (action === 'start') {
      setStatus((current) => ({
        ...current,
        state: 'starting',
        lastError: null,
      }));
    }
    try {
      const next = action === 'start'
        ? await startGoRouteIsolated()
        : await stopGoRouteIsolated();
      if (shouldApplyGoRouteResult(actionGeneration, generationRef.current, false)) {
        setStatus(next);
      }
    } catch {
      if (shouldApplyGoRouteResult(actionGeneration, generationRef.current, false)) {
        setStatus(failedGoIsolatedStatus);
      }
    } finally {
      if (actionGeneration === generationRef.current) {
        busyRef.current = false;
        setBusy(false);
      }
    }
  }, [status.state]);

  const disabled = busy || status.state === 'starting';

  return (
    <Card className="p-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-medium">{t('routes.board.goIsolatedTitle')}</p>
            <Badge variant={goIsolatedBadgeVariant(status.state)}>
              {goIsolatedStatusLabel(status.state, t)}
            </Badge>
          </div>
          <p className="mt-1 text-xs text-secondary">{t('routes.board.goIsolatedHint')}</p>
          {status.state !== 'stopped' ? (
            <p className="mt-1 text-xs text-secondary">
              {t('routes.board.goIsolatedRuntime', {
                inFlight: status.inFlightCount,
                healthy: status.healthyMemberCount,
                members: status.memberCount,
                restarts: status.restartCount,
              })}
            </p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            disabled={disabled}
            onClick={() => {
              void run('start');
            }}
          >
            {t('routes.board.goIsolatedStart')}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            disabled={disabled}
            onClick={() => {
              void run('stop');
            }}
          >
            {t('routes.board.goIsolatedStop')}
          </Button>
        </div>
      </div>
      {status.state === 'failed' ? (
        <div className="mt-2">
          <AdapterErrorLines
            error={status.lastError}
            fallback={t('routes.board.goIsolatedFailed')}
          />
        </div>
      ) : null}
    </Card>
  );
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
  const keyLabel = t('routes.board.endpointLogins', {
    count: row.keyCount,
  });
  const color = agentCssVar(localEndpointBrandAgentId(row.kind));
  const agentIds = supportedAgentsForEndpointKind(row.kind);

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
        'cursor-pointer p-3 transition-colors hover:border-accent/40 hover:bg-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        selected && 'border-accent bg-hover/40',
        row.keyCount === 0 && 'opacity-70',
      )}
    >
      <Tip className="block min-w-0 truncate font-mono text-sm font-medium" label={row.path}>
        <span style={{ color }}>{row.path}</span>
      </Tip>
      <p className="mt-1 text-xs text-secondary">{label}</p>
      {agentIds.length > 0 ? (
        <div className="mt-2 flex flex-wrap items-center gap-1">
          {agentIds.map((agentId) => (
            <AgentLogo key={agentId} agentId={agentId} size="sm" />
          ))}
        </div>
      ) : null}
      <p className="mt-1 text-xs text-muted">{keyLabel}</p>
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
  const { t, lang } = useI18n();
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
  const {
    chatCompletionsShared,
    defaultPools,
    loading: poolsLoading,
  } = useRoutePoolState({
    profiles,
    detailTarget: null,
    reloadKey: usageRefreshKey,
  });
  const [tokensByPoolId, setTokensByPoolId] = useState<Record<string, string>>({});
  const [tokenRecords, setTokenRecords] = useState<LocalTokenRecord[] | null>(null);
  const {
    profileErrors,
    busyProfileIds,
    handleStartLocalGateway,
    handleStopLocalGateway,
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
  const [localGatewayRestarting, setLocalGatewayRestarting] = useState(false);
  useEffect(() => {
    void getLocalGatewayStatus()
      .then((status) => {
        setGatewayRunning(status.running);
        setLocalGatewayRestarting(status.restarting);
        for (const row of status.statuses) updateBridgeStatus(row);
      })
      .catch(() => undefined);
  }, [updateBridgeStatus, usageRefreshKey]);
  useEffect(() => {
    let cancelled = false;
    void listLocalTokens()
      .then((records) => {
        if (cancelled) return;
        const next: Record<string, string> = {};
        for (const record of records) next[record.poolId] = record.token;
        setTokensByPoolId(next);
        setTokenRecords(records);
      })
      .catch(() => {
        if (!cancelled) {
          setTokensByPoolId({});
          setTokenRecords([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [defaultPools, usageRefreshKey]);
  const localGateway = useMemo(
    () => buildLocalGatewayControl(
      profiles,
      bridgeStatuses,
      hiddenTargetIds,
      defaultPools,
      localGatewayRestarting,
    ),
    [bridgeStatuses, defaultPools, hiddenTargetIds, localGatewayRestarting, profiles],
  );
  const localGatewayBusy = localGateway.profileIds.some((id) => busyProfileIds[id])
    || Boolean(busyProfileIds.__local_gateway__);
  const localGatewayError = profileErrors.__local_gateway__
    ?? localGateway.profileIds.map((id) => profileErrors[id]).find((error) => error != null)
    ?? null;
  const localGatewayRetryAction = resolveLocalGatewayRetryAction(
    localGateway,
    localGatewayError != null,
  );
  const retryLocalGateway = useCallback(() => {
    if (localGatewayRetryAction === 'stop') {
      setStopOpen(true);
      return;
    }
    if (localGatewayRetryAction !== 'start') return;
    void handleStartLocalGateway().then((ok) => setGatewayRunning(ok));
  }, [handleStartLocalGateway, localGatewayRetryAction]);

  const tokenRows = useMemo(
    () => buildLocalTokenRows(
      profiles,
      bridgeStatuses,
      errors.bridgeStatuses,
      defaultPools,
      chatCompletionsShared,
      tokensByPoolId,
      tokenRecords,
    ),
    [
      bridgeStatuses,
      chatCompletionsShared,
      defaultPools,
      errors.bridgeStatuses,
      profiles,
      tokenRecords,
      tokensByPoolId,
    ],
  );
  const endpointRows = useMemo(
    () => buildBoardEndpointTypeRows(
      visibleTokenKinds(tokenRows.filter((row) => Boolean(row.token)), hiddenTargetIds),
    ),
    [hiddenTargetIds, tokenRows],
  );
  useEffect(() => {
    setEndpointKind((selected) => visibleBoardEndpointKind(selected, endpointRows));
  }, [endpointRows]);
  const totals = boardEndpointKeyTotals(endpointRows);
  const usageSurface = endpointKind === 'all'
    ? 'all'
    : poolSurfaceToUsageSurface(localEndpointSurface(endpointKind));
  const selectedRow = endpointKind === 'all'
    ? null
    : endpointRows.find((row) => row.kind === endpointKind) ?? null;
  const fleetLabel = totals.keys > 0
    ? t('routes.board.fleetLogins', { count: totals.keys })
    : t('routes.board.description');
  const keyHint = selectedRow
    ? t('routes.board.endpointLoginsHint', {
        agents: supportedAgentsForEndpointKind(selectedRow.kind)
          .map((id) => agentDisplayName(id))
          .join(lang === 'en' ? ', ' : '、'),
      })
    : t('routes.board.endpointLoginsHintAll', { count: totals.keys });
  const pageLoading = loading || poolsLoading;
  const showStatusSkeleton = pageLoading && defaultPools.length === 0;
  const entryRunning = localGateway.running || gatewayRunning;
  const entryLabel = localGatewayStatusLabel(
    { ...localGateway, running: entryRunning, action: entryRunning ? 'stop' : localGateway.action },
    t,
  );
  const entryBadge = localGateway.profileIds.length === 0
    ? null
    : (
      <Badge variant={entryRunning ? 'success' : 'default'}>
        {entryLabel}
      </Badge>
    );

  return (
    <RoutesPane>
      <div className={pageRhythm.overviewColumn}>
      <PageHeader
        title={t('routes.board.title')}
        badge={entryBadge}
        description={fleetLabel}
        descriptionTip={t('routes.board.descriptionTip')}
      />

      {import.meta.env.DEV ? <GoIsolatedStrip /> : null}

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
          {localGatewayError ? (
            <div className="space-y-2">
              <AdapterErrorLines
                error={localGatewayError}
                fallback={localGateway.action === 'stop'
                  ? t('routes.board.entryStopFailed')
                  : t('routes.board.entryStartFailed')}
              />
              {localGatewayRetryAction ? (
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={localGatewayBusy || localGateway.transitioning}
                  onClick={retryLocalGateway}
                >
                  {t('chrome.error.retry')}
                </Button>
              ) : null}
            </div>
          ) : null}
          <PageSection
            first
            data-help="routes-board-endpoints"
            title={t('routes.board.statusSection')}
            actions={
              <div className="flex items-center gap-2">
                <span className="text-meta text-secondary">{t('routes.board.entrySwitch')}</span>
                <Hint
                  label={
                    localGateway.running || localGateway.action === 'start'
                      ? undefined
                      : entryLabel
                  }
                >
                  <Switch
                    data-help="routes-board-switch"
                    checked={entryRunning}
                    disabled={localGatewayBusy || localGateway.transitioning}
                    onCheckedChange={(on) => {
                      if (on) {
                        void handleStartLocalGateway().then((ok) => setGatewayRunning(ok));
                      } else {
                        setStopOpen(true);
                      }
                    }}
                    aria-label={t('routes.board.entrySwitch')}
                  />
                </Hint>
              </div>
            }
          >
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
            <p className="mt-3 text-sm text-secondary">{keyHint}</p>
          </PageSection>

          <div data-help="routes-board-usage">
          <BoardUsageSection
            profiles={profiles}
            hiddenTargetIds={hiddenTargetIds}
            pools={defaultPools}
            refreshKey={usageRefreshKey}
            surface={usageSurface}
            forwardingOn={entryRunning}
            headerActions={
              <PageRefreshButton
                loading={pageLoading}
                onClick={() => {
                  void reload();
                  setUsageRefreshKey((key) => key + 1);
                }}
                label={t('routes.board.refresh')}
              />
            }
          />
          </div>
        </div>
      )}

      <Dialog
        open={stopOpen}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, localGatewayBusy, () => setStopOpen(false))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={localGatewayBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(localGatewayBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(localGatewayBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(localGatewayBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.board.entryStopTitle')}</DialogTitle>
            <DialogDescription>{t('routes.board.entryStopDescription')}</DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button
              variant="secondary"
              onClick={() => setStopOpen(false)}
              disabled={localGatewayBusy}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={localGatewayBusy}
              onClick={() => {
                void handleStopLocalGateway().then((ok) => {
                  if (ok) {
                    setGatewayRunning(false);
                    setStopOpen(false);
                  }
                });
              }}
            >
              {localGatewayBusy ? t('routes.stop.confirming') : t('routes.stop.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      </div>
    </RoutesPane>
  );
}
