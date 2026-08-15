import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { Button } from '@/components/ui/button';
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
  adapterPageDescription,
  resourceFailureMessage,
} from './adapter-model';
import {
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  filterBoundLocalBridgeRuntimes,
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
  filterBoundLocalBridgeRuntimes,
  resolveAdapterProfileSource,
} from './adapter-view-model';

export {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';

/**
 * Adapter page: bound local-bridge runtimes only.
 * Creating bindings lives in Dashboard and Connections ConnectFlow.
 * Do not mount analyze fan-out, plan, or apply-confirm hooks here.
 */
export default function AdapterPage() {
  const navigate = useNavigate();
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
  const [bindingProfileIds, setBindingProfileIds] = useState<ReadonlySet<string>>(new Set());
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
      .then((wallet) => {
        if (cancelled) return;
        setBindingProfileIds(new Set(
          wallet.bindings
            .map((binding) => binding.profileId)
            .filter((id): id is string => typeof id === 'string' && id.length > 0),
        ));
      })
      .catch(() => {
        if (!cancelled) setBindingProfileIds(new Set());
      });
    return () => {
      cancelled = true;
    };
  }, [profiles]);

  const boundBridgeProfiles = useMemo(
    () => filterBoundLocalBridgeRuntimes(profiles, { entries, bindingProfileIds }),
    [bindingProfileIds, entries, profiles],
  );

  const connectionWarning = resourceFailureMessage(resourceErrors);
  const stopError = stopConfirm ? profileErrors[stopConfirm.id] : null;
  const removeError = removeConfirm ? profileErrors[removeConfirm.id] : null;
  const stopDialogBusy = Boolean(stopConfirm && busyProfileIds[stopConfirm.id]);
  const removeDialogBusy = removingProfileId !== null;
  const fleetSummary = adapterBridgeFleetSummary(boundBridgeProfiles, bridgeStatuses);

  const detailProfile = detailProfileId
    ? profiles.find((profile) => profile.id === detailProfileId) ?? null
    : null;

  return (
    <div>
      <PageHeader
        title="桥与适配"
        description={adapterPageDescription()}
        descriptionTip="凭据保存在 Connections，不会展示或复制。本地桥接只监听 127.0.0.1，日志不记请求正文。"
        actions={(
          <>
            <Button onClick={() => navigate('/')}>去 Dashboard 连接</Button>
            <Button variant="outline" onClick={() => navigate('/connections')}>
              去 Connections
            </Button>
          </>
        )}
      />

      {connectionWarning && <p className="mb-3 text-sm text-warning" role="alert">{connectionWarning}</p>}

      <PageSection
        title="本机桥运行时"
        description="只列出已绑定的本机桥。创建绑定不在本页，请走 Dashboard 或 Connections。"
      >
        {fleetSummary ? (
          <p className="mb-3 text-xs text-secondary">
            {fleetSummary.label} · 需保持托盘运行。
          </p>
        ) : null}
        <AdapterProfiles
          profiles={boundBridgeProfiles}
          bridgeStatuses={bridgeStatuses}
          statusErrors={resourceErrors.bridgeStatuses}
          entries={entries}
          loading={loading && profiles.length === 0}
          loadError={profileState === 'error' ? resourceErrors.profiles : null}
          errors={profileErrors}
          busyProfileIds={busyProfileIds}
          removingProfileId={removingProfileId}
          onStartBridge={handleStartBridge}
          onRequestStopBridge={setStopConfirm}
          onShowDetail={(profile) => setDetailProfileId(profile.id)}
          onRetry={() => void reload()}
          onStartCreate={() => navigate('/')}
        />
      </PageSection>

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
            <DialogTitle>停止本地桥接？</DialogTitle>
            <DialogDescription>停止后，目标 Agent 将无法访问此桥接。</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {stopConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(stopConfirm, entries)}</p>}
            {stopError ? <AdapterErrorLines error={stopError} fallback="无法停止本地桥接" /> : null}
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
            <DialogTitle>删除此适配？</DialogTitle>
            <DialogDescription>
              会解除这条绑定并停桥。来源票仍留在钱包。
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {removeConfirm && <p className="text-sm text-secondary">{adapterProfileFlowLabel(removeConfirm, entries)}</p>}
            {removeError ? <AdapterErrorLines error={removeError} fallback="无法删除此适配" /> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button variant="secondary" onClick={() => setRemoveConfirm(null)} disabled={removeDialogBusy}>取消</Button>
            <Button variant="danger" onClick={() => void confirmRemove()} disabled={removeDialogBusy}>
              {removeDialogBusy ? '删除中…' : removeError ? '重试删除' : '确认删除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
