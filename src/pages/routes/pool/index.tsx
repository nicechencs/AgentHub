import { useEffect, useMemo, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Boxes, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { Tip } from '@/components/ui/tooltip';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { useToast } from '@/components/ui/toast';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { AdapterErrorLines, AdapterProfiles } from '@/pages/bridges/adapter-components';
import {
  adapterBridgeHostPort,
  resolveBridgesProfileQuery,
  resourceFailureMessage,
} from '@/pages/bridges/adapter-model';
import {
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  bridgesPageViewState,
  groupLocalBridgeProfiles,
  isLocalBridgeCardActive,
  partitionLocalBridgeRuntimes,
} from '@/pages/bridges/adapter-view-model';
import { EditRouteDialog } from '@/pages/bridges/EditRouteDialog';
import { RouteDetailPanel } from '@/pages/bridges/RouteDetailPanel';
import { WriteClientConfigDialog } from '@/pages/bridges/WriteClientConfigDialog';
import { buildRouteGraph } from '@/pages/bridges/route-graph-model';
import {
  inspectProfileId,
  liveInspectProfile,
  ROUTES_INSPECT_WIDTH_KEY,
  type RouteInspect,
} from '@/pages/bridges/route-inspect';
import {
  buildPoolWorkbenchRows,
  directProfilesForRoutePoolV2,
  matchDefaultPoolForProfile,
} from '@/pages/bridges/route-pool-view-model';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useBridgeRuntimeActions } from '@/pages/bridges/use-bridge-runtime-actions';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import { useOAuthLoginAgents } from '@/pages/connections/use-oauth-login-agents';
import { PoolAddButtons } from './PoolAddButtons';
import { PoolCard } from './PoolCard';

/**
 * 授权池工作台：本机入口、已接入登录、写进客户端、解绑。
 * 登录增删走连接页。
 */
export default function RoutesPoolPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams] = useSearchParams();
  const profileQuery = searchParams.get('profile');
  const openedProfileQueryRef = useRef<string | null>(null);
  const {
    entries,
    profiles,
    bridgeStatuses,
    errors: resourceErrors,
    profileState,
    loading,
    wallet,
    reload,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
  } = useAdapterResources();
  const { hiddenIds, installedIds, visibleIds, loading: agentsLoading } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const allowedAgents = installedIds.length > 0 || !agentsLoading ? installedIds : visibleIds;
  const oauthLoginAgents = useOAuthLoginAgents(allowedAgents);
  const inspect = useSideSplit<RouteInspect>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });

  const runtime = useBridgeRuntimeActions({
    profiles,
    hiddenTargetIds,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    t,
    toast,
    onEnrollDone: () => inspect.close(),
  });
  const {
    removeConfirm,
    setRemoveConfirm,
    stopConfirm,
    setStopConfirm,
    removingProfileId,
    profileErrors,
    busyProfileIds,
    enrollingProfileId,
    handleStartBridge,
    confirmStopBridge,
    confirmRemove,
    handleEnrollNative,
  } = runtime;

  useEffect(() => {
    if (loading && profiles.length === 0) return;
    if (openedProfileQueryRef.current === profileQuery) return;
    const resolvedId = resolveBridgesProfileQuery(profileQuery, profiles);
    openedProfileQueryRef.current = profileQuery;
    if (!resolvedId) return;
    const profile = profiles.find((row) => row.id === resolvedId);
    if (!profile) return;
    inspect.open({ kind: 'detail', profile });
  }, [inspect.open, loading, profileQuery, profiles]);

  const inspectTarget = inspect.target;
  const detailTarget = inspectTarget?.kind === 'detail'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;

  const { routePoolV2, defaultPools, nativeCanApplyById } = useRoutePoolState({
    profiles,
    detailTarget,
  });

  const { bound, orphan } = useMemo(
    () => partitionLocalBridgeRuntimes(profiles, {
      entries,
      bindingProfileIds: wallet.bindingProfileIds,
    }),
    [entries, profiles, wallet.bindingProfileIds],
  );
  const groupedOrphan = useMemo(
    () => groupLocalBridgeProfiles(orphan, bridgeStatuses),
    [orphan, bridgeStatuses],
  );
  const rows = useMemo(
    () => buildPoolWorkbenchRows({
      flagOn: routePoolV2,
      pools: defaultPools,
      profiles: bound,
      statuses: bridgeStatuses,
    }),
    [bound, bridgeStatuses, defaultPools, routePoolV2],
  );
  const orphanIds = useMemo(
    () => new Set(groupedOrphan.map((profile) => profile.id)),
    [groupedOrphan],
  );
  const visibleRows = useMemo(
    () => rows.filter((row) => {
      if (!row.profile) return true;
      return !orphanIds.has(row.profile.id);
    }),
    [orphanIds, rows],
  );
  const directProfiles = useMemo(
    () => directProfilesForRoutePoolV2(routePoolV2, profiles, bridgeStatuses),
    [bridgeStatuses, profiles, routePoolV2],
  );

  const inspectProfile = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : inspectTarget?.kind === 'write'
      ? liveInspectProfile(inspectTarget.target.profile, profiles)
      : detailTarget;
  const detailPool = inspectProfile && routePoolV2
    ? matchDefaultPoolForProfile(defaultPools, inspectProfile)
    : null;
  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;
  const removeConfirmIsOrphan = Boolean(
    removeConfirm && orphan.some((profile) => profile.id === removeConfirm.id),
  );
  const fleetSummary = adapterBridgeFleetSummary(
    [...bound, ...orphan],
    bridgeStatuses,
    t,
  );
  const orphanOnly = visibleRows.length === 0 && groupedOrphan.length > 0;
  const hasContent = visibleRows.length > 0 || groupedOrphan.length > 0 || directProfiles.length > 0;
  const pageView = bridgesPageViewState({
    profileState: loading && profileState !== 'error' ? 'loading' : profileState,
    bound: visibleRows,
    orphan: groupedOrphan,
    wallet: {
      settled: wallet.settled,
      lastWalletBridgeCount: wallet.lastWalletBridgeCount,
    },
  });
  const writeTarget = inspectTarget?.kind === 'write' ? inspectTarget.target : null;
  const editTarget = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;
  const activeProfileId = inspectProfileId(inspectTarget);

  const listProps = {
    bridgeStatuses,
    statusErrors: resourceErrors.bridgeStatuses,
    entries,
    errors: profileErrors,
    busyProfileIds,
    removingProfileId,
    onStartBridge: handleStartBridge,
    onRequestStopBridge: setStopConfirm,
    onRequestWrite: (profile: AdapterProfile, graph: ReturnType<typeof buildRouteGraph>) => {
      inspect.open({ kind: 'write', target: { profile, graph } });
    },
    onRequestEdit: (profile: AdapterProfile) => {
      inspect.open({ kind: 'edit', profile });
    },
    onShowDetail: (profile: AdapterProfile) => {
      inspect.open({ kind: 'detail', profile });
    },
    activeProfileId,
    onRetry: () => { void reload(); },
    hiddenTargetIds,
    siblingProfiles: profiles,
  };

  const openWrite = (profile: AdapterProfile) => {
    const status = profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined;
    const endpointParts = adapterBridgeHostPort(profile, status);
    const graph = buildRouteGraph({
      profile,
      entries,
      siblingProfiles: profiles,
      host: endpointParts?.host,
      port: endpointParts?.port,
    });
    inspect.open({ kind: 'write', target: { profile, graph } });
  };

  const inspectPanel =
    inspectTarget?.kind === 'write' && writeTarget ? (
      <WriteClientConfigDialog
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        profile={writeTarget.profile}
        rows={writeTarget.graph.rows}
        host={writeTarget.graph.local.host}
        port={writeTarget.graph.local.port ?? null}
        sourceMissing={writeTarget.graph.source.missing}
        listedModels={writeTarget.graph.listedModels}
        contextWindowTokens={writeTarget.graph.contextWindowTokens}
        localToken={bridgeStatuses[writeTarget.profile.id]?.localToken}
        siblingProfiles={profiles}
        hiddenTargetIds={hiddenTargetIds}
        onWritten={() => { void reload(); }}
      />
    ) : inspectTarget?.kind === 'edit' && editTarget ? (
      <EditRouteDialog
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        profile={editTarget}
        entries={entries}
        busy={busyProfileIds[editTarget.id] === true}
        onSaved={() => { void reload(); }}
        onRequestDelete={setRemoveConfirm}
      />
    ) : inspectTarget?.kind === 'detail' && detailTarget ? (
      <RouteDetailPanel
        id={`route-detail-${detailTarget.id}`}
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        profile={detailTarget}
        bridgeStatus={detailTarget.route === 'local_bridge' ? bridgeStatuses[detailTarget.id] : undefined}
        entries={entries}
        siblingProfiles={profiles}
        busy={busyProfileIds[detailTarget.id] === true || removingProfileId === detailTarget.id}
        error={profileErrors[detailTarget.id]}
        onRequestRemove={setRemoveConfirm}
        onRequestEdit={(profile) => inspect.open({ kind: 'edit', profile })}
        targetHidden={hiddenTargetIds.has(detailTarget.targetAgentId)}
        routePoolV2={routePoolV2}
        defaultPool={detailPool}
        canApplyLocalBridge={nativeCanApplyById[detailTarget.id] === true}
        onEnrollNative={handleEnrollNative}
        enrolling={enrollingProfileId === detailTarget.id}
      />
    ) : null;

  return (
    <>
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
      >
        <PageHeader
          title={t('routes.pool.page.title')}
          description={t('routes.pool.page.description')}
        />
        <div className={pageRhythm.chromeRow}>
          {connectionWarning ? (
            <Notice tone="warning" className="min-w-0 flex-1 items-center py-1">
              {connectionWarning}
            </Notice>
          ) : orphanOnly ? (
            <Tip className="min-w-0 truncate text-meta text-secondary" label={t('routes.orphan.description')}>
              {t('routes.orphan.title')}
            </Tip>
          ) : fleetSummary ? (
            <p className="min-w-0 truncate text-meta text-secondary">{fleetSummary.label}</p>
          ) : (
            <p className="min-w-0 truncate text-meta text-muted">
              {t('routes.pool.page.chromeHint')}
            </p>
          )}
          <div className={pageRhythm.chromeActions}>
            <PoolAddButtons agents={allowedAgents} oauthAgents={oauthLoginAgents} />
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={loading}
              onClick={() => void reload()}
            >
              {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              {t('routes.board.refresh')}
            </Button>
          </div>
        </div>

        <div className={pageRhythm.stackDense}>
          {pageView === 'loading' ? (
            <div className="space-y-2" aria-live="polite">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          ) : null}
          {pageView === 'list_error' ? (
            <ErrorState
              title={t('routes.loadError')}
              error={resourceErrors.profiles ?? new Error(t('routes.loadError'))}
              onRetry={() => { void reload(); }}
            />
          ) : null}
          {pageView === 'wallet_without_runtime' ? (
            <ErrorState
              title={t('routes.walletWithoutRuntime.title')}
              error={new Error(t('routes.walletWithoutRuntime.description'))}
              onRetry={() => { void reload(); }}
            />
          ) : null}
          {pageView === 'healthy_empty' && !hasContent ? (
            <EmptyState
              icon={Boxes}
              title={t('routes.pool.page.emptyTitle')}
              description={t('routes.pool.page.emptyDescription')}
            />
          ) : null}
          {pageView === 'list' || (pageView === 'healthy_empty' && hasContent) ? (
            <>
              {visibleRows.length > 0 ? (
                <div className="space-y-2">
                  {visibleRows.map((row) => {
                    const profile = row.profile;
                    return (
                      <PoolCard
                        key={row.key}
                        row={row}
                        entries={entries}
                        bridgeStatus={
                          profile?.route === 'local_bridge'
                            ? bridgeStatuses[profile.id]
                            : undefined
                        }
                        statusUnavailable={Boolean(
                          profile && resourceErrors.bridgeStatuses[profile.id],
                        )}
                        busy={Boolean(
                          profile
                          && (busyProfileIds[profile.id] === true || removingProfileId === profile.id),
                        )}
                        error={profile ? profileErrors[profile.id] : undefined}
                        active={Boolean(
                          profile
                          && isLocalBridgeCardActive(profile, activeProfileId, profiles),
                        )}
                        targetHidden={Boolean(
                          profile && hiddenTargetIds.has(profile.targetAgentId),
                        )}
                        onStart={(item) => void handleStartBridge(item)}
                        onStop={setStopConfirm}
                        onWrite={openWrite}
                        onShowDetail={(item) => inspect.open({ kind: 'detail', profile: item })}
                      />
                    );
                  })}
                </div>
              ) : null}
              {groupedOrphan.length > 0 ? (
                <PageSection
                  first={orphanOnly}
                  title={orphanOnly ? undefined : t('routes.orphan.title')}
                  description={orphanOnly ? undefined : t('routes.orphan.description')}
                >
                  <AdapterProfiles
                    {...listProps}
                    profiles={groupedOrphan}
                    loading={false}
                    loadError={null}
                  />
                </PageSection>
              ) : null}
              {directProfiles.length > 0 ? (
                <PageSection
                  first={visibleRows.length === 0 && groupedOrphan.length === 0}
                  title={t('routes.direct.title')}
                  description={t('routes.direct.description')}
                >
                  <AdapterProfiles
                    {...listProps}
                    onRequestWrite={undefined}
                    onRequestEdit={undefined}
                    profiles={directProfiles}
                    loading={false}
                    loadError={null}
                  />
                </PageSection>
              ) : null}
            </>
          ) : null}
        </div>
      </WorkbenchSplitPage>

      <Dialog
        open={Boolean(stopConfirm)}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, stopDialogBusy, () => setStopConfirm(null))}
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
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(stopConfirm, entries)}</p>}
            {stopError ? <AdapterErrorLines error={stopError} fallback={t('routes.stop.fallback')} /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setStopConfirm(null)} disabled={stopDialogBusy}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => void confirmStopBridge()} disabled={stopDialogBusy}>
              {stopDialogBusy ? t('routes.stop.confirming') : t('routes.stop.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(removeConfirm)}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, removeDialogBusy, () => setRemoveConfirm(null))}
      >
        <DialogContent
          className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
          hideClose={removeDialogBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(removeDialogBusy, event)}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.unbind.title')}</DialogTitle>
            <DialogDescription>
              {t(removeConfirmIsOrphan ? 'routes.unbind.orphanDescription' : 'routes.unbind.description')}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(removeConfirm, entries)}</p>}
            {removeError ? <AdapterErrorLines error={removeError} fallback={t('routes.unbind.fallback')} /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setRemoveConfirm(null)} disabled={removeDialogBusy}>{t('common.cancel')}</Button>
            <Button variant="danger" onClick={() => void confirmRemove()} disabled={removeDialogBusy}>
              {removeDialogBusy ? t('routes.unbind.confirming') : removeError ? t('routes.unbind.retry') : t('routes.unbind.confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
