import { useEffect, useMemo, useState } from 'react';
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
import {
  enrollNativeToGateway,
  listDefaultRoutePools,
  planAdapter,
  startAdapterBridge,
  stopAdapterBridge,
} from '@/lib/api/adapter';
import { listTicketWallet, ticketIdFor, unbindTicket } from '@/lib/api/tickets';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
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
  localBridgeProfilesForSource,
  localBridgeSourceKey,
  partitionLocalBridgeRuntimes,
} from './adapter-view-model';
import {
  directProfilesForRoutePoolV2,
  matchDefaultPoolForProfile,
} from './route-pool-view-model';
import { useAdapterResources } from './use-bridge-resources';
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

type WalletSnapshot = {
  settled: boolean;
  lastWalletBridgeCount: number;
  bindingProfileIds: ReadonlySet<string>;
};

/** One route plus the endpoint graph its client-config write is derived from. */
type WriteTarget = { profile: AdapterProfile; graph: RouteGraphView };

type RouteInspect =
  | { kind: 'create' }
  | { kind: 'import' }
  | { kind: 'write'; target: WriteTarget }
  | { kind: 'edit'; profile: AdapterProfile }
  | { kind: 'detail'; profile: AdapterProfile };

function inspectProfileId(target: RouteInspect | null): string | null {
  if (!target) return null;
  if (target.kind === 'edit' || target.kind === 'detail') return target.profile.id;
  if (target.kind === 'write') return target.target.profile.id;
  return null;
}

function liveInspectProfile(
  snapshot: AdapterProfile,
  profiles: readonly AdapterProfile[],
): AdapterProfile {
  return profiles.find((profile) => profile.id === snapshot.id) ?? snapshot;
}

const ROUTES_INSPECT_WIDTH_KEY = 'agenthub.routes.inspectWidth';

/**
 * Local-bridge runtime ops page. Creating bindings lives in Dashboard and
 * Connections ConnectFlow. Do not mount analyze fan-out, plan, or apply here.
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
    reload,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
  } = useAdapterResources();
  const { hiddenIds } = useInstalledAgents();
  const hiddenTargetIds = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const [wallet, setWallet] = useState<WalletSnapshot>({
    settled: false,
    lastWalletBridgeCount: 0,
    bindingProfileIds: new Set(),
  });
  const [removeConfirm, setRemoveConfirm] = useState<AdapterProfile | null>(null);
  const [stopConfirm, setStopConfirm] = useState<AdapterProfile | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, unknown>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});
  const inspect = useSideSplit<RouteInspect>({ storageKey: ROUTES_INSPECT_WIDTH_KEY });
  const routeOrder = useStoredIdOrder(StorageKey.routesProfileOrder);
  const [routePoolV2, setRoutePoolV2] = useState(false);
  const [defaultPools, setDefaultPools] = useState<DefaultRoutePoolOverview[]>([]);
  const [nativeCanApplyById, setNativeCanApplyById] = useState<Record<string, boolean>>({});
  const [enrollingProfileId, setEnrollingProfileId] = useState<string | null>(null);

  const setProfileBusy = (profileId: string, busy: boolean) => {
    setBusyProfileIds((current) => ({ ...current, [profileId]: busy }));
  };

  const clearProfileError = (profileId: string) => {
    setProfileErrors((current) => {
      const { [profileId]: _ignored, ...remaining } = current;
      return remaining;
    });
  };

  const reloadThenClearProfileErrors = () => {
    void reloadProfiles().then(
      () => { setProfileErrors({}); },
      () => undefined,
    );
  };

  const handleStartBridge = async (profile: AdapterProfile) => {
    const members = localBridgeProfilesForSource(profiles, profile)
      .filter((member) => !hiddenTargetIds.has(member.targetAgentId));
    if (members.length === 0) return;
    for (const member of members) {
      setProfileBusy(member.id, true);
      clearProfileError(member.id);
    }
    try {
      for (const member of members) {
        updateBridgeStatus(await startAdapterBridge(member.id));
      }
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      for (const member of members) setProfileBusy(member.id, false);
    }
  };

  const confirmStopBridge = async () => {
    if (!stopConfirm) return;
    const profile = stopConfirm;
    const members = localBridgeProfilesForSource(profiles, profile);
    for (const member of members) {
      setProfileBusy(member.id, true);
      clearProfileError(member.id);
    }
    try {
      for (const member of members) {
        updateBridgeStatus(await stopAdapterBridge(member.id));
      }
      setStopConfirm(null);
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      for (const member of members) setProfileBusy(member.id, false);
    }
  };

  const confirmRemove = async () => {
    if (!removeConfirm || hiddenTargetIds.has(removeConfirm.targetAgentId)) return;
    const profile = removeConfirm;
    const members = localBridgeProfilesForSource(profiles, profile);
    const profileId = profile.id;
    setRemovingProfileId(profileId);
    clearProfileError(profileId);
    try {
      const wallet = await listTicketWallet();
      for (const member of members) {
        const binding = wallet.bindings.find((row) => row.profileId === member.id);
        const ticketId = binding?.ticketId ?? ticketIdFor(member.sourceKind, member.sourceId);
        const agentId = binding?.agentId ?? member.targetAgentId;
        await unbindTicket(ticketId, agentId);
        removeProfile(member.id);
      }
      setRemoveConfirm(null);
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((errors) => ({ ...errors, [profileId]: error }));
    } finally {
      setRemovingProfileId(null);
    }
  };

  useEffect(() => {
    let cancelled = false;
    void listTicketWallet()
      .then((next) => {
        if (cancelled) return;
        setWallet({
          settled: true,
          lastWalletBridgeCount: next.bindings.filter((binding) => binding.route === 'bridge').length,
          bindingProfileIds: new Set(
            next.bindings
              .map((binding) => binding.profileId)
              .filter((id): id is string => typeof id === 'string' && id.length > 0),
          ),
        });
      })
      .catch(() => {
        if (cancelled) return;
        setWallet((current) => ({ ...current, settled: true }));
      });
    return () => {
      cancelled = true;
    };
  }, [profiles]);

  useEffect(() => {
    let cancelled = false;
    void listDefaultRoutePools()
      .then((listed) => {
        if (cancelled) return;
        setRoutePoolV2(listed.enabled);
        setDefaultPools(listed.pools);
      })
      .catch(() => {
        if (cancelled) return;
        setRoutePoolV2(false);
        setDefaultPools([]);
      });
    return () => {
      cancelled = true;
    };
  }, [profiles]);

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

  const inspectTarget = inspect.target;
  const writeTarget = inspectTarget?.kind === 'write' ? inspectTarget.target : null;
  const editTarget = inspectTarget?.kind === 'edit'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;
  const detailTarget = inspectTarget?.kind === 'detail'
    ? liveInspectProfile(inspectTarget.profile, profiles)
    : null;
  const activeProfileId = inspectProfileId(inspectTarget);
  const directProfiles = useMemo(
    () => directProfilesForRoutePoolV2(routePoolV2, profiles),
    [routePoolV2, profiles],
  );
  const detailPool = detailTarget && routePoolV2
    ? matchDefaultPoolForProfile(defaultPools, detailTarget)
    : null;

  useEffect(() => {
    if (!routePoolV2 || !detailTarget) return;
    if (detailTarget.route !== 'native_endpoint' && detailTarget.route !== 'config_sync') return;
    const profileId = detailTarget.id;
    let cancelled = false;
    void planAdapter({
      sourceKind: detailTarget.sourceKind,
      sourceId: detailTarget.sourceId,
      targetAgentId: detailTarget.targetAgentId,
    })
      .then((plan) => {
        if (cancelled) return;
        setNativeCanApplyById((current) => ({
          ...current,
          [profileId]: plan.canApply && plan.analysis.route === 'local_bridge',
        }));
      })
      .catch(() => {
        if (cancelled) return;
        setNativeCanApplyById((current) => ({ ...current, [profileId]: false }));
      });
    return () => {
      cancelled = true;
    };
  }, [routePoolV2, detailTarget]);

  const handleEnrollNative = async (profile: AdapterProfile) => {
    setEnrollingProfileId(profile.id);
    clearProfileError(profile.id);
    try {
      await enrollNativeToGateway(profile.id);
      toast({ title: t('routes.pool.enrollSuccess'), variant: 'success' });
      inspect.close();
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setEnrollingProfileId(null);
    }
  };

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
      if (
        inspect.target?.kind === 'detail'
        && localBridgeSourceKey(inspect.target.profile) === localBridgeSourceKey(profile)
      ) {
        inspect.close();
        return;
      }
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
