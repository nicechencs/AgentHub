import { useEffect, useMemo } from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { useStoredIdOrder } from '@/components/shared/use-stored-id-order';
import { applyIdOrder } from '@/lib/list-order';
import { StorageKey } from '@/lib/ui-preferences';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { Button } from '@/components/ui/button';
import { Boxes } from 'lucide-react';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { useToast } from '@/components/ui/toast';
import { AdapterErrorLines, AdapterProfiles } from './adapter-components';
import { CreateRouteDialog } from './CreateRouteDialog';
import { EditRouteDialog } from './EditRouteDialog';
import { ImportRouteDialog } from './ImportRouteDialog';
import { RouteDetailPanel } from './RouteDetailPanel';
import { WriteClientConfigDialog } from './WriteClientConfigDialog';
import type { RouteGraphView } from './route-graph-model';
import {
  resourceFailureMessage,
} from './adapter-model';
import {
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  bridgesPageViewState,
  canonicalizeLocalBridgeOrderIds,
  groupLocalBridgeProfiles,
  localBridgeSourceKey,
  partitionLocalBridgeRuntimes,
} from './adapter-view-model';
import {
  directProfilesForRoutePoolV2,
  matchDefaultPoolForProfile,
} from './route-pool-view-model';
import { useAdapterResources } from './use-bridge-resources';
import { useBridgeRuntimeActions } from './use-bridge-runtime-actions';
import { useRoutePoolState } from './use-route-pool-state';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  inspectProfileId,
  liveInspectProfile,
  ROUTES_INSPECT_WIDTH_KEY,
  type RouteInspect,
} from './route-inspect';

/**
 * Routes（本机转发）页：bridge 运行时 ops 为主；创建/导入路由并 bind 为产品例外
 * （亦可在 Dashboard / Connections ConnectFlow 完成）。不在此挂载 ConnectFlow
 * analyze fan-out；native enroll 预览走 planTicket。
 */
export default function BridgesPage() {
  const { t } = useI18n();
  const { toast } = useToast();
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
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const inspect = useSideSplit<RouteInspect>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const routeOrder = useStoredIdOrder(StorageKey.routesProfileOrder);

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
  const groupedBound = useMemo(
    () => groupLocalBridgeProfiles(bound, bridgeStatuses),
    [bound, bridgeStatuses],
  );
  const groupedOrphan = useMemo(
    () => groupLocalBridgeProfiles(orphan, bridgeStatuses),
    [orphan, bridgeStatuses],
  );
  const routeOrderIds = useMemo(
    () => canonicalizeLocalBridgeOrderIds(routeOrder.stored, profiles),
    [profiles, routeOrder.stored],
  );
  const orderedBound = useMemo(
    () => applyIdOrder(groupedBound, localBridgeSourceKey, routeOrderIds),
    [groupedBound, routeOrderIds],
  );
  const orderedOrphan = useMemo(
    () => applyIdOrder(groupedOrphan, localBridgeSourceKey, routeOrderIds),
    [groupedOrphan, routeOrderIds],
  );

  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;
  const listedBridges = useMemo(
    () => [...groupedBound, ...groupedOrphan],
    [groupedBound, groupedOrphan],
  );
  const fleetSummary = adapterBridgeFleetSummary(listedBridges, bridgeStatuses, t);
  const pageView = bridgesPageViewState({
    profileState: loading && profileState !== 'error' ? 'loading' : profileState,
    bound: groupedBound,
    orphan: groupedOrphan,
    wallet: {
      settled: wallet.settled,
      lastWalletBridgeCount: wallet.lastWalletBridgeCount,
    },
  });

  const removeConfirmIsOrphan = Boolean(
    removeConfirm && orphan.some((profile) => profile.id === removeConfirm.id),
  );

  const writeTarget = inspectTarget?.kind === 'write' ? inspectTarget.target : null;
  const editTarget = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;
  const activeProfileId = inspectProfileId(inspectTarget);
  const directProfiles = useMemo(
    () => directProfilesForRoutePoolV2(routePoolV2, profiles, bridgeStatuses),
    [routePoolV2, profiles, bridgeStatuses],
  );
  const detailPool = detailTarget && routePoolV2
    ? matchDefaultPoolForProfile(defaultPools, detailTarget)
    : null;

  const listProps = {
    bridgeStatuses,
    statusErrors: resourceErrors.bridgeStatuses,
    entries,
    errors: profileErrors,
    busyProfileIds,
    removingProfileId,
    onStartBridge: handleStartBridge,
    onRequestStopBridge: setStopConfirm,
    onRequestWrite: (profile: AdapterProfile, graph: RouteGraphView) => {
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
  const boundIds = useMemo(
    () => orderedBound.map(localBridgeSourceKey),
    [orderedBound],
  );
  const orphanIds = useMemo(
    () => orderedOrphan.map(localBridgeSourceKey),
    [orderedOrphan],
  );
  const moveBound = (fromId: string, toId: string) => {
    const from = orderedBound.find((profile) => profile.id === fromId);
    const to = orderedBound.find((profile) => profile.id === toId);
    if (!from || !to) return;
    routeOrder.moveInLive(boundIds, localBridgeSourceKey(from), localBridgeSourceKey(to));
  };
  const moveOrphan = (fromId: string, toId: string) => {
    const from = orderedOrphan.find((profile) => profile.id === fromId);
    const to = orderedOrphan.find((profile) => profile.id === toId);
    if (!from || !to) return;
    routeOrder.moveInLive(orphanIds, localBridgeSourceKey(from), localBridgeSourceKey(to));
  };

  useEffect(() => {
    routeOrder.seedIfEmpty([...boundIds, ...orphanIds]);
  }, [boundIds, orphanIds, routeOrder.seedIfEmpty]);

  const inspectPanel =
    inspectTarget?.kind === 'create' ? (
      <CreateRouteDialog
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        onCreated={() => { void reload(); }}
      />
    ) : inspectTarget?.kind === 'import' ? (
      <ImportRouteDialog
        asPanel
        open
        width={inspect.paneWidth}
        onOpenChange={(open) => { if (!open) inspect.close(); }}
        entries={entries}
        profiles={profiles}
        bindingProfileIds={wallet.bindingProfileIds}
        onImported={() => { void reload(); }}
      />
    ) : inspectTarget?.kind === 'write' && writeTarget ? (
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
      header={(
        <PageHeader
          size="compact"
          title={t('routes.page.title')}
          description={t('routes.page.description')}
          descriptionTip={t('routes.page.descriptionTip')}
          actions={
            <div className="flex gap-2">
              <Button variant="secondary" onClick={() => inspect.open({ kind: 'import' })}>
                {t('routes.import.action')}
              </Button>
              <Button onClick={() => inspect.open({ kind: 'create' })}>
                {t('routes.create.action')}
              </Button>
            </div>
          }
        />
      )}
    >
      {connectionWarning ? (
        <div className={pageRhythm.lead}>
          <Notice tone="warning">{connectionWarning}</Notice>
        </div>
      ) : null}

      <div className={pageRhythm.stackDense}>
        {pageView === 'loading' ? (
          <AdapterProfiles
            {...listProps}
            profiles={[]}
            loading
            loadError={null}
          />
        ) : null}
        {pageView === 'list_error' ? (
          <AdapterProfiles
            {...listProps}
            profiles={[]}
            loading={false}
            loadError={resourceErrors.profiles ?? new Error(t('routes.loadError'))}
          />
        ) : null}
        {pageView === 'wallet_without_runtime' ? (
          <ErrorState
            title={t('routes.walletWithoutRuntime.title')}
            error={new Error(t('routes.walletWithoutRuntime.description'))}
            onRetry={() => { void reload(); }}
          />
        ) : null}
        {pageView === 'healthy_empty' && directProfiles.length === 0 ? (
          <EmptyState
            icon={Boxes}
            title={t('routes.empty.title')}
            description={t('routes.empty.description')}
          />
        ) : null}
        {pageView === 'list' || (pageView === 'healthy_empty' && directProfiles.length > 0) ? (
          <>
            {fleetSummary ? (
              <p className="text-xs text-secondary">{fleetSummary.label}</p>
            ) : null}
            {orderedBound.length > 0 ? (
              <AdapterProfiles
                {...listProps}
                profiles={orderedBound}
                onMove={moveBound}
                loading={false}
                loadError={null}
              />
            ) : null}
            {orderedOrphan.length > 0 ? (
              <PageSection
                title={t('routes.orphan.title')}
                description={t('routes.orphan.description')}
              >
                <AdapterProfiles
                  {...listProps}
                  profiles={orderedOrphan}
                  onMove={moveOrphan}
                  loading={false}
                  loadError={null}
                />
              </PageSection>
            ) : null}
            {directProfiles.length > 0 ? (
              <PageSection
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
