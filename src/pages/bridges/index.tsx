import { useEffect, useMemo, useState } from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { Button } from '@/components/ui/button';
import { Boxes } from 'lucide-react';
import {
  startAdapterBridge,
  stopAdapterBridge,
} from '@/lib/api/adapter';
import { listTicketWallet, ticketIdFor, unbindTicket } from '@/lib/api/tickets';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { AdapterErrorLines, AdapterProfiles } from './adapter-components';
import { CreateRouteDialog } from './CreateRouteDialog';
import { EditRouteDialog } from './EditRouteDialog';
import { ImportRouteDialog } from './ImportRouteDialog';
import { WriteClientConfigDialog } from './WriteClientConfigDialog';
import type { RouteGraphView } from './route-graph-model';
import {
  resourceFailureMessage,
} from './adapter-model';
import {
  adapterBridgeFleetSummary,
  adapterProfileFlowLabel,
  bridgesPageViewState,
  partitionLocalBridgeRuntimes,
} from './adapter-view-model';
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

/**
 * Local-bridge runtime ops page. Creating bindings lives in Dashboard and
 * Connections ConnectFlow. Do not mount analyze fan-out, plan, or apply here.
 */
export default function BridgesPage() {
  const { t } = useI18n();
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
  const [createOpen, setCreateOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [writeTarget, setWriteTarget] = useState<WriteTarget | null>(null);
  const [editTarget, setEditTarget] = useState<AdapterProfile | null>(null);

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
    if (hiddenTargetIds.has(profile.targetAgentId)) return;
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

  const confirmRemove = async () => {
    if (!removeConfirm || hiddenTargetIds.has(removeConfirm.targetAgentId)) return;
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
  const fleetSummary = adapterBridgeFleetSummary(listedBridges, bridgeStatuses, t);
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

  const listProps = {
    bridgeStatuses,
    statusErrors: resourceErrors.bridgeStatuses,
    entries,
    errors: profileErrors,
    busyProfileIds,
    removingProfileId,
    onStartBridge: handleStartBridge,
    onRequestStopBridge: setStopConfirm,
    onRequestRemove: setRemoveConfirm,
    onRequestWrite: (profile: AdapterProfile, graph: RouteGraphView) => {
      setCreateOpen(false);
      setImportOpen(false);
      setEditTarget(null);
      setWriteTarget({ profile, graph });
    },
    onRequestEdit: (profile: AdapterProfile) => {
      setCreateOpen(false);
      setImportOpen(false);
      setWriteTarget(null);
      setEditTarget(profile);
    },
    onRetry: () => { void reload(); },
    hiddenTargetIds,
  };

  return (
    <div>
      <PageHeader
        title={t('routes.page.title')}
        description={t('routes.page.description')}
        descriptionTip={t('routes.page.descriptionTip')}
        actions={
          <div className="flex gap-2">
            <Button variant="secondary" onClick={() => {
              setCreateOpen(false);
              setWriteTarget(null);
              setEditTarget(null);
              setImportOpen(true);
            }}>{t('routes.import.action')}</Button>
            <Button onClick={() => {
              setImportOpen(false);
              setWriteTarget(null);
              setEditTarget(null);
              setCreateOpen(true);
            }}>{t('routes.create.action')}</Button>
          </div>
        }
      />

      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
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
        {pageView === 'healthy_empty' ? (
          <EmptyState
            icon={Boxes}
            title={t('routes.empty.title')}
            description={t('routes.empty.description')}
            actionLabel={t('routes.create.action')}
            onAction={() => setCreateOpen(true)}
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
                title={t('routes.orphan.title')}
                description={t('routes.orphan.description')}
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

        </div>
      <CreateRouteDialog
        asPanel
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={() => { void reload(); }}
      />
      <ImportRouteDialog
        asPanel
        open={importOpen}
        onOpenChange={setImportOpen}
        entries={entries}
        profiles={profiles}
        bindingProfileIds={wallet.bindingProfileIds}
        onImported={() => { void reload(); }}
      />
      <WriteClientConfigDialog
        asPanel
        open={Boolean(writeTarget)}
        onOpenChange={(open) => { if (!open) setWriteTarget(null); }}
        profile={writeTarget?.profile ?? null}
        rows={writeTarget?.graph.rows ?? []}
        host={writeTarget?.graph.local.host}
        port={writeTarget?.graph.local.port ?? null}
        sourceMissing={writeTarget?.graph.source.missing ?? false}
        hiddenTargetIds={hiddenTargetIds}
        onWritten={() => { void reload(); }}
      />
      <EditRouteDialog
        asPanel
        open={Boolean(editTarget)}
        onOpenChange={(open) => { if (!open) setEditTarget(null); }}
        profile={editTarget}
        entries={entries}
        busy={editTarget ? busyProfileIds[editTarget.id] === true : false}
        onSaved={() => { void reload(); }}
        onRequestDelete={setRemoveConfirm}
      />
      </div>

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
              {t('routes.unbind.description')}
              {removeConfirmIsOrphan ? t('routes.unbind.orphanNote') : ''}
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
    </div>
  );
}
