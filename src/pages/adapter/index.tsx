import { useEffect, useMemo, useState } from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Button } from '@/components/ui/button';
import { Boxes } from 'lucide-react';
import {
  setAdapterBridgeAutoStart,
  startAdapterBridge,
  stopAdapterBridge,
} from '@/lib/api/adapter';
import { listTicketWallet, ticketIdFor, unbindTicket } from '@/lib/api/tickets';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { AdapterErrorLines, AdapterProfiles } from './adapter-components';
import { AdapterProfileDetailDialog } from './AdapterProfileDetailDialog';
import {
  BRIDGES_EMPTY_DESCRIPTION,
  BRIDGES_EMPTY_TITLE,
  BRIDGES_PAGE_DESCRIPTION,
  BRIDGES_PAGE_DESCRIPTION_TIP,
  BRIDGES_PAGE_TITLE,
  BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION,
  BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE,
  resourceFailureMessage,
} from './adapter-model';
import {
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  bridgesPageViewState,
  partitionLocalBridgeRuntimes,
} from './adapter-view-model';
import { useAdapterResources } from './use-adapter-resources';
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

export {
  adapterActionLabel,
  adapterAgentBadgeStyle,
  adapterApplyCommit,
  adapterBridgeEndpointLabel,
  adapterBridgeStateLabel,
  adapterBridgeUpstreamLabel,
  adapterCredentialFilterLabel,
  adapterCredentialKindLabel,
  adapterPageDescription,
  BRIDGES_EMPTY_DESCRIPTION,
  BRIDGES_EMPTY_TITLE,
  BRIDGES_MUTATION_FAILURE,
  BRIDGES_NAV_LABEL,
  BRIDGES_PAGE_DESCRIPTION,
  BRIDGES_PAGE_DESCRIPTION_TIP,
  BRIDGES_PAGE_TITLE,
  BRIDGES_PATH,
  BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION,
  BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE,
  adapterPageViewState,
  adapterPlanChangeLabel,
  adapterPlanRequestSignature,
  adapterPreviewOutcome,
  adapterProfileRecordLabel,
  adapterProfileStatusLabel,
  adapterServiceImpactLabel,
  adapterTabDescription,
  adapterTabLabel,
  adapterTableRouteLabel,
  canApplyAdapterPlan,
  canApplyAdapterSelection,
  canConfirmAdapterApply,
  canRequestAdapterPlan,
  connectionKindForFilter,
  connectionKindForTab,
  filterProfilesByCredential,
  filterProfilesByMode,
  isAdapterPlanMatchedToSelection,
  isCurrentAdapterPreviewRequest,
  isSameAdapterPlanRequestSignature,
  isSubscriptionGateUnsupported,
  maskedIdSuffix,
  parseAdapterCredentialFilter,
  parseAdapterTab,
  resolveAdapterTargetAgentId,
  resolveAdapterVisibleSourceKey,
  routeLabel,
  sourceLabel,
  sourceStatusHint,
  unsupportedPresentation,
} from './adapter-model';

export {
  adapterApplySummaryLine,
  adapterBridgeFleetSummary,
  adapterConfigStatusView,
  adapterProfileFlowLabel,
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterRoutePipelineModel,
  adapterServiceStatusView,
  adapterTargetBadge,
  adapterTargetCacheKey,
  bridgesPageViewState,
  bridgeRuntimeStatusView,
  filterBoundLocalBridgeRuntimes,
  partitionLocalBridgeRuntimes,
  resolveAdapterProfileSource,
} from './adapter-view-model';

export {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';

type WalletSnapshot = {
  settled: boolean;
  lastWalletBridgeCount: number;
  bindingProfileIds: ReadonlySet<string>;
};

/**
 * Local-bridge runtime ops page. Creating bindings lives in Dashboard and
 * Connections ConnectFlow. Do not mount analyze fan-out, plan, or apply here.
 */
export default function AdapterPage() {
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
    updateProfile,
    removeProfile,
  } = useAdapterResources();
  const [wallet, setWallet] = useState<WalletSnapshot>({
    settled: false,
    lastWalletBridgeCount: 0,
    bindingProfileIds: new Set(),
  });
  const [removeConfirm, setRemoveConfirm] = useState<AdapterProfile | null>(null);
  const [stopConfirm, setStopConfirm] = useState<AdapterProfile | null>(null);
  const [detailProfileId, setDetailProfileId] = useState<string | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, unknown>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});

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
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateBridgeStatus(await startAdapterBridge(profile.id));
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const confirmStopBridge = async () => {
    if (!stopConfirm) return;
    const profile = stopConfirm;
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateBridgeStatus(await stopAdapterBridge(profile.id));
      setStopConfirm(null);
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const handleSetBridgeAutoStart = async (profile: AdapterProfile, autoStart: boolean) => {
    setProfileBusy(profile.id, true);
    clearProfileError(profile.id);
    try {
      updateProfile(await setAdapterBridgeAutoStart(profile.id, autoStart));
      reloadThenClearProfileErrors();
    } catch (error) {
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setProfileBusy(profile.id, false);
    }
  };

  const confirmRemove = async () => {
    if (!removeConfirm) return;
    const profile = removeConfirm;
    const profileId = profile.id;
    setRemovingProfileId(profileId);
    clearProfileError(profileId);
    try {
      const wallet = await listTicketWallet();
      const binding = wallet.bindings.find((row) => row.profileId === profile.id);
      const ticketId = binding?.ticketId ?? ticketIdFor(profile.sourceKind, profile.sourceId);
      const agentId = binding?.agentId ?? profile.targetAgentId;
      await unbindTicket(ticketId, agentId);
      removeProfile(profileId);
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

  const { bound, orphan } = useMemo(
    () => partitionLocalBridgeRuntimes(profiles, {
      entries,
      bindingProfileIds: wallet.bindingProfileIds,
    }),
    [entries, profiles, wallet.bindingProfileIds],
  );

  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;
  const listedBridges = useMemo(() => [...bound, ...orphan], [bound, orphan]);
  const fleetSummary = adapterBridgeFleetSummary(listedBridges, bridgeStatuses);
  const pageView = bridgesPageViewState({
    profileState: loading && profileState !== 'error' ? 'loading' : profileState,
    bound,
    orphan,
    wallet: {
      settled: wallet.settled,
      lastWalletBridgeCount: wallet.lastWalletBridgeCount,
    },
  });
  const removeConfirmIsOrphan = Boolean(
    removeConfirm && orphan.some((profile) => profile.id === removeConfirm.id),
  );

  const detailProfile = detailProfileId
    ? profiles.find((profile) => profile.id === detailProfileId) ?? null
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
    onShowDetail: (profile: AdapterProfile) => setDetailProfileId(profile.id),
    onRetry: () => { void reload(); },
  };

  return (
    <div>
      <PageHeader
        title={BRIDGES_PAGE_TITLE}
        description={BRIDGES_PAGE_DESCRIPTION}
        descriptionTip={BRIDGES_PAGE_DESCRIPTION_TIP}
      />

      {connectionWarning && <p className="mb-3 text-sm text-warning" role="alert">{connectionWarning}</p>}

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
            loadError={resourceErrors.profiles ?? new Error('无法读取本机桥')}
          />
        ) : null}
        {pageView === 'wallet_without_runtime' ? (
          <ErrorState
            title={BRIDGES_WALLET_WITHOUT_RUNTIME_TITLE}
            error={new Error(BRIDGES_WALLET_WITHOUT_RUNTIME_DESCRIPTION)}
            onRetry={() => { void reload(); }}
          />
        ) : null}
        {pageView === 'healthy_empty' ? (
          <EmptyState
            icon={Boxes}
            title={BRIDGES_EMPTY_TITLE}
            description={BRIDGES_EMPTY_DESCRIPTION}
          />
        ) : null}
        {pageView === 'list' ? (
          <>
            {fleetSummary ? (
              <p className="text-xs text-secondary">{fleetSummary.label}</p>
            ) : null}
            {bound.length > 0 ? (
              <AdapterProfiles
                {...listProps}
                profiles={bound}
                loading={false}
                loadError={null}
              />
            ) : null}
            {orphan.length > 0 ? (
              <PageSection
                title="孤立本机桥"
                description="来源票或绑定记录已不在。停止或解除仍走同一套命令。"
              >
                <AdapterProfiles
                  {...listProps}
                  profiles={orphan}
                  loading={false}
                  loadError={null}
                />
              </PageSection>
            ) : null}
          </>
        ) : null}
      </div>

      <AdapterProfileDetailDialog
        profile={detailProfile}
        bridgeStatus={detailProfile ? bridgeStatuses[detailProfile.id] : undefined}
        statusUnavailable={detailProfile ? Boolean(resourceErrors.bridgeStatuses[detailProfile.id]) : false}
        entries={entries}
        busy={detailProfile
          ? busyProfileIds[detailProfile.id] === true || removingProfileId === detailProfile.id
          : false}
        error={detailProfile ? profileErrors[detailProfile.id] : null}
        onClose={() => setDetailProfileId(null)}
        onSetAutoStart={handleSetBridgeAutoStart}
        onRequestRemove={(profile) => {
          setDetailProfileId(null);
          setRemoveConfirm(profile);
        }}
      />

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
            <DialogTitle>停止本机桥？</DialogTitle>
            <DialogDescription>停止后，该工具将无法通过此转发访问上游。</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(stopConfirm, entries)}</p>}
            {stopError ? <AdapterErrorLines error={stopError} fallback="无法停止本机桥" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setStopConfirm(null)} disabled={stopDialogBusy}>取消</Button>
            <Button variant="danger" onClick={() => void confirmStopBridge()} disabled={stopDialogBusy}>
              {stopDialogBusy ? '停止中…' : '确认停止'}
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
            <DialogTitle>解除本机桥绑定？</DialogTitle>
            <DialogDescription>
              会停桥并恢复该工具上一份配置。票仍留在 Connections。
              {removeConfirmIsOrphan ? '来源或绑定记录已不在，仍走同一解除。' : ''}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(removeConfirm, entries)}</p>}
            {removeError ? <AdapterErrorLines error={removeError} fallback="无法解除本机桥绑定" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setRemoveConfirm(null)} disabled={removeDialogBusy}>取消</Button>
            <Button variant="danger" onClick={() => void confirmRemove()} disabled={removeDialogBusy}>
              {removeDialogBusy ? '解除中…' : removeError ? '重试解除' : '确认解除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
