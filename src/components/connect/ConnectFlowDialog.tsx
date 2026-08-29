import * as React from 'react';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useAgentStatusesOptional } from '@/app/runtime';
import { useConnectionPool } from '@/app/runtime/ConnectionPoolProvider';
import { AGENT_IDS, agentDisplayName } from '@/config/agents';
import { hiddenAgentIdSet } from '@/lib/agent-visibility';
import {
  formatLocalRouteLabel,
  isInternalGeneratedProvider,
  resolveEffectiveConnection,
} from '@/lib/api/agent-connection';
import type { AdapterProfile } from '@/lib/api/adapter';
import { buildConnectionsGuideUrl } from '@/lib/connect-flow/connect-intent';
import type { AgentId } from '@/lib/types';
import type {
  ConnectFlowDialogProps,
  PlanEligibility,
  PlanFanoutController,
} from '@/lib/connect-flow/types';
import {
  beginConfirm,
  canBuildSourceOptions,
  canConfirm,
  canEnterPreview,
  canRetry,
  connectFlowEntryKey,
  createConnectFlowState,
  currentTargetAgentId,
  eligibilityOf,
  excludeOwnAgentTargets,
  keepOwnAgentTarget,
  fanoutRequestsForAgent,
  fanoutRequestsForSource,
  findOption,
  guideTargetAgentId,
  isConnectFlowEntryStale,
  isGeneratedAdapterSource,
  isPreviewInvalid,
  isProfilesReadyForEntry,
  notifyConnectionChangedSafe,
  reduceConnectFlow,
  releaseConfirmLock,
  resolveEmptyKind,
  settleConfirm,
  shouldRevertPreviewToSelect,
  shouldShowPreviewImportHint,
  sourceAgentIdOf,
  tryAcquireConfirmLock,
  visibleTargetsForPurpose,
} from './connect-flow-state';
import {
  ConnectFlowSelectStep,
  EffectiveSummary,
  FixedSourceSummary,
  SelectLoadingSkeleton,
} from './ConnectFlowSelectStep';
import { ConnectFlowPreviewStep } from './ConnectFlowPreviewStep';
import { ConnectFlowResultStep } from './ConnectFlowResultStep';

const EMPTY_ELIGIBILITY: ReadonlyMap<string, PlanEligibility> = new Map();

function uniquePoolAgentIds(accounts: { agentId: AgentId }[], providers: { agentId: AgentId }[]): AgentId[] {
  const ids = new Set<AgentId>();
  for (const item of accounts) ids.add(item.agentId);
  for (const item of providers) ids.add(item.agentId);
  return [...ids];
}

export function ConnectFlowDialog({
  entry,
  deps,
  onClose,
  onConnectionChanged,
  onNavigate,
  asPanel = false,
  width,
}: ConnectFlowDialogProps) {
  const open = entry !== null;
  const key = connectFlowEntryKey(entry);
  const { t } = useI18n();
  const pool = useConnectionPool();
  const { statuses } = useAgentStatusesOptional();
  const depsRef = React.useRef(deps);
  depsRef.current = deps;
  const onChangedRef = React.useRef(onConnectionChanged);
  onChangedRef.current = onConnectionChanged;

  const [state, dispatch] = React.useReducer(
    reduceConnectFlow,
    entry ?? { mode: 'for-agent', targetAgentId: '_' },
    createConnectFlowState,
  );
  const [profiles, setProfiles] = React.useState<AdapterProfile[]>([]);
  const [profilesError, setProfilesError] = React.useState<unknown>(null);
  const [profilesLoadedKey, setProfilesLoadedKey] = React.useState<string | null>(null);
  const [fanout, setFanout] = React.useState<PlanFanoutController | null>(null);
  const sessionRef = React.useRef(0);
  const confirmingRef = React.useRef(false);
  const profilesReady = isProfilesReadyForEntry(profilesLoadedKey, key);
  const optionsReady = canBuildSourceOptions(profilesReady, profilesError);
  const entryStale = isConnectFlowEntryStale(state.entry, entry);

  React.useEffect(() => {
    if (!entry) return;
    dispatch({ type: 'reset', entry });
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps -- reset only when the opened entry identity changes

  React.useEffect(() => {
    sessionRef.current += 1;
    if (!open) {
      setFanout(null);
      return () => {
        sessionRef.current += 1;
      };
    }
    const controller = depsRef.current.createPlanFanout();
    controller.invalidate();
    setFanout(controller);
    return () => {
      sessionRef.current += 1;
      controller.cancel();
    };
  }, [open, key]);

  React.useEffect(() => {
    if (!open) return;
    void pool.ensureLoaded();
  }, [open, pool.ensureLoaded]);

  const loadProfiles = React.useCallback(async (loadKey: string | null) => {
    const session = sessionRef.current;
    setProfilesError(null);
    setProfilesLoadedKey(null);
    try {
      const list = await depsRef.current.listProfiles();
      if (session !== sessionRef.current) return;
      setProfiles(list);
      setProfilesLoadedKey(loadKey);
    } catch (error) {
      if (session !== sessionRef.current) return;
      setProfiles([]);
      setProfilesError(error);
      setProfilesLoadedKey(loadKey);
    }
  }, []);

  React.useEffect(() => {
    if (!open) {
      setProfiles([]);
      setProfilesError(null);
      setProfilesLoadedKey(null);
      return;
    }
    void loadProfiles(key);
  }, [open, key, loadProfiles]);

  const eligibilities = React.useSyncExternalStore(
    React.useCallback((listener) => (fanout ? fanout.subscribe(listener) : () => {}), [fanout]),
    React.useCallback(() => (fanout ? fanout.getState() : EMPTY_ELIGIBILITY), [fanout]),
    React.useCallback(() => EMPTY_ELIGIBILITY, []),
  );

  const options = React.useMemo(() => {
    if (!entry || entry.mode !== 'for-agent' || !optionsReady) return [];
    return deps.buildSourceOptions({
      targetAgentId: entry.targetAgentId,
      accounts: pool.accounts,
      providers: pool.providers,
      profiles,
      agentStatuses: statuses,
      t,
    });
  }, [entry, deps, pool.accounts, pool.providers, profiles, optionsReady, statuses, t]);

  const sourceAgentId = entry
    ? sourceAgentIdOf(entry, pool.accounts, pool.providers)
    : null;
  // 引用必须稳定：这些数组直接进 effect 依赖，逐渲染新建会反复触发 fanout.start
  const hiddenSet = React.useMemo(() => hiddenAgentIdSet(statuses ?? []), [statuses]);
  const catalogIds = React.useMemo(() => {
    const ids = AGENT_IDS.length > 0 ? [...AGENT_IDS] : uniquePoolAgentIds(pool.accounts, pool.providers);
    return ids.filter((id) => !hiddenSet.has(id));
  }, [pool.accounts, pool.providers, hiddenSet]);
  const keepOwnAgent = keepOwnAgentTarget(entry, pool.accounts);
  const allTargetAgentIds = React.useMemo(
    () => (entry?.mode === 'for-source'
      ? excludeOwnAgentTargets(catalogIds, sourceAgentId, keepOwnAgent)
      : []),
    [entry, catalogIds, sourceAgentId, keepOwnAgent],
  );
  const targetAgentIds = React.useMemo(
    () => (entry?.mode === 'for-source'
      ? visibleTargetsForPurpose(
          allTargetAgentIds,
          entry.source,
          eligibilities,
          entry.purpose,
        )
      : []),
    [entry, allTargetAgentIds, eligibilities],
  );

  const generatedSourceBlocked = Boolean(
    entry?.mode === 'for-source'
    && profilesReady
    && isGeneratedAdapterSource(entry.source, profiles),
  );

  const fanoutRequests = React.useMemo(() => {
    if (!entry || !optionsReady || generatedSourceBlocked) return [];
    return entry.mode === 'for-agent'
      ? fanoutRequestsForAgent(options, entry.targetAgentId)
      : fanoutRequestsForSource(entry.source, allTargetAgentIds);
  }, [entry, options, allTargetAgentIds, optionsReady, generatedSourceBlocked]);

  React.useEffect(() => {
    if (!fanout || !entry) return;
    if (pool.state === 'idle' || pool.state === 'loading') return;
    if (fanoutRequests.length === 0) return;
    // start() 对相同请求集合幂等（controller 内签名判重），依赖抖动不会打断在途请求
    fanout.start(fanoutRequests, { accounts: pool.accounts });
  }, [fanout, entry, fanoutRequests, pool.accounts, pool.state]);

  React.useEffect(() => {
    if (!entry) return;
    if (shouldRevertPreviewToSelect({
      step: state.step,
      mode: entry.mode,
      selectedSource: state.selectedSource,
      options,
    })) {
      dispatch({ type: 'back_to_select' });
    }
  }, [entry, state.step, state.selectedSource, options]);

  const busy = !entryStale && state.busy !== 'idle';
  const selectedOption = findOption(options, state.selectedSource);
  const previewInvalid = isPreviewInvalid({
    state,
    options,
    accounts: pool.accounts,
    providers: pool.providers,
  });
  const targetId = entry ? currentTargetAgentId(state) : null;
  const selectedEligibility = eligibilityOf(
    eligibilities,
    entry?.mode === 'for-source' ? entry.source : state.selectedSource,
    targetId,
  );
  const emptyKind = entry
    ? resolveEmptyKind({
        poolState: pool.state,
        poolErrors: pool.errors,
        profilesError,
        profilesReady,
        profiles,
        accounts: pool.accounts,
        providers: pool.providers,
        options,
        eligibilities,
        entry,
        visibleTargetAgentIds: targetAgentIds,
        t,
      })
    : { kind: 'none' as const };

  const currentAccount = targetId
    ? pool.accounts.find((item) => item.agentId === targetId && item.isCurrent)
    : undefined;
  const currentProvider = targetId
    ? pool.providers.find((item) => item.agentId === targetId && item.isCurrent)
    : undefined;
  const generatedProfile = currentProvider
    ? profiles.find((profile) => profile.generatedProviderId === currentProvider.id)
    : undefined;
  const generatedSourceLabel = generatedProfile
    ? generatedProfile.sourceKind === 'account'
      ? pool.accounts.find((item) => item.id === generatedProfile.sourceId)?.email
        ?? pool.accounts.find((item) => item.id === generatedProfile.sourceId)?.label
      : pool.providers.find((item) => item.id === generatedProfile.sourceId)?.name
    : undefined;
  const resolved = resolveEffectiveConnection(currentAccount, currentProvider, {
    t,
    sourceLabel: generatedSourceLabel,
  });
  const effective = resolved.kind === 'api'
    && currentProvider
    && (generatedProfile || isInternalGeneratedProvider(currentProvider))
    ? {
        ...resolved,
        label: formatLocalRouteLabel(generatedSourceLabel, t),
      }
    : resolved;

  const requestClose = React.useCallback(() => {
    if (state.busy !== 'idle') return;
    onClose();
  }, [state.busy, onClose]);

  const navigateTo = React.useCallback((to: string) => {
    if (state.busy !== 'idle') return;
    onClose();
    onNavigate(to);
  }, [state.busy, onClose, onNavigate]);

  const guideAgent = (entry && guideTargetAgentId(state)) || (entry?.mode === 'for-agent' ? entry.targetAgentId : null);

  const goImportLogin = React.useCallback(() => {
    if (!guideAgent) {
      navigateTo('/connections');
      return;
    }
    navigateTo(buildConnectionsGuideUrl({
      agentId: guideAgent,
      intent: 'import-login',
      resumeAgentId: guideAgent,
    }));
  }, [guideAgent, navigateTo]);

  const handleConfirm = React.useCallback(() => {
    if (previewInvalid) return;
    if (!tryAcquireConfirmLock(confirmingRef)) return;
    const begun = beginConfirm(state);
    if (!begun.allowed) {
      releaseConfirmLock(confirmingRef);
      return;
    }
    dispatch(state.previewKind === 'switch' ? { type: 'begin_switch' } : { type: 'begin_apply' });
    const session = sessionRef.current;
    void (async () => {
      try {
        const settled = await settleConfirm({
          state: begun.next,
          startedGeneration: begun.next.selectionGeneration,
          deps: depsRef.current,
          option: selectedOption,
          t,
        });
        if (session !== sessionRef.current) return;
        dispatch(settled.event);
        if (settled.event.type !== 'apply_succeeded' && settled.event.type !== 'switch_succeeded') return;
        if (session !== sessionRef.current) return;
        const outcome = settled.event.type === 'apply_succeeded'
          ? { kind: 'applied' as const, result: settled.event.result }
          : { kind: 'switched' as const, ref: settled.event.ref, agentId: settled.event.agentId };
        const ok = await notifyConnectionChangedSafe(outcome, onChangedRef.current);
        if (session !== sessionRef.current) return;
        if (!ok) dispatch({ type: 'refresh_failed' });
      } finally {
        releaseConfirmLock(confirmingRef);
      }
    })();
  }, [state, selectedOption, previewInvalid, t]);

  const retryResources = React.useCallback(() => {
    void pool.reload();
    void loadProfiles(key);
  }, [pool, loadProfiles, key]);

  const title = !entry
    ? t('connect.dialog.title')
    : entry.mode === 'for-agent'
      ? t('connect.dialog.titleAgent', { name: agentDisplayName(entry.targetAgentId) })
      : entry.purpose === 'route'
        ? t('connect.dialog.titleRoute')
        : entry.purpose === 'share'
          ? t('connect.dialog.titleShare')
          : t('connect.dialog.titleSource');

  const summary = !entry ? null : entry.mode === 'for-agent' ? (
    <EffectiveSummary
      agentId={entry.targetAgentId}
      label={effective.label}
      authLabel={effective.authHealthLabel}
    />
  ) : (
    <FixedSourceSummary
      entry={entry}
      accounts={pool.accounts}
      providers={pool.providers}
    />
  );

  const steps = !entry ? null : (
    <>
      {entryStale ? (
        <SelectLoadingSkeleton />
      ) : state.step === 'select' ? (
        <ConnectFlowSelectStep
          entry={entry}
          state={state}
          options={options}
          eligibilities={eligibilities}
          targetAgentIds={targetAgentIds}
          sourceAgentId={sourceAgentId}
          emptyKind={emptyKind}
          poolLoading={pool.state === 'idle' || pool.state === 'loading'}
          profilesReady={profilesReady}
          onSelectSource={(option) => dispatch({ type: 'select_source', option })}
          onSelectTarget={(agentId) => dispatch({
            type: 'select_target',
            agentId,
            sourceAgentId,
            allowOwnAgent: keepOwnAgent,
          })}
          onRetryEligibility={(request) => fanout?.retry(request)}
          onRetryResources={retryResources}
          onGoImport={goImportLogin}
          onOauthGuide={(agentId) => navigateTo(`/connections?agent=${agentId}`)}
        />
      ) : null}

      {!entryStale && state.step === 'preview' ? (
        <ConnectFlowPreviewStep
          state={state}
          option={selectedOption}
          previewInvalid={previewInvalid}
          previewNative={deps.previewNative}
          onGoImport={goImportLogin}
          showImportHint={shouldShowPreviewImportHint({
            entry,
            option: selectedOption,
            accounts: pool.accounts,
            providers: pool.providers,
          })}
        />
      ) : null}

      {!entryStale && state.step === 'result' && state.result ? (
        <ConnectFlowResultStep result={state.result} />
      ) : null}
    </>
  );

  const actionSize = asPanel ? 'sm' : undefined;
  const actions = !entry ? null : (
    <>
      {entryStale || state.step === 'select' ? (
        <>
          <Button type="button" size={actionSize} variant="secondary" disabled={busy} onClick={requestClose}>
            {t('common.cancel')}
          </Button>
          {!entryStale ? (
            <Button
              type="button"
              size={actionSize}
              disabled={busy || !canEnterPreview(state, selectedOption, selectedEligibility)}
              onClick={() => dispatch({
                type: 'enter_preview',
                option: selectedOption,
                eligibility: selectedEligibility,
              })}
            >
              {t('connect.dialog.next')}
            </Button>
          ) : null}
        </>
      ) : null}
      {!entryStale && state.step === 'preview' ? (
        <>
          <Button
            type="button"
            size={actionSize}
            variant="secondary"
            disabled={busy}
            onClick={() => dispatch({ type: 'back_to_select' })}
          >
            {t('connect.dialog.back')}
          </Button>
          <Button
            type="button"
            size={actionSize}
            disabled={!canConfirm(state) || previewInvalid}
            onClick={handleConfirm}
          >
            {busy
              ? (state.busy === 'switching' ? t('connect.dialog.switching') : t('connect.dialog.applying'))
              : (state.previewKind === 'switch' ? t('connect.dialog.confirmSwitch') : t('connect.dialog.confirmApply'))}
          </Button>
        </>
      ) : null}
      {!entryStale && state.step === 'result' ? (
        <>
          {canRetry(state) ? (
            <Button type="button" size={actionSize} onClick={() => dispatch({ type: 'retry_from_result' })}>
              {t('chrome.error.retry')}
            </Button>
          ) : null}
          <Button type="button" size={actionSize} variant="secondary" onClick={requestClose}>
            {t('connect.dialog.close')}
          </Button>
        </>
      ) : null}
    </>
  );

  if (asPanel) {
    if (!open || !entry) return null;
    return (
      <SideInspectPanel
        title={title}
        onClose={requestClose}
        headerActions={actions}
        width={width}
      >
        <div className="space-y-4">
          {summary}
          {steps}
        </div>
      </SideInspectPanel>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => closeConfirmationOnOpenChange(next, busy, requestClose)}
    >
      <DialogContent
        className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
        hideClose={busy}
        onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
        onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        onInteractOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
      >
        {entry ? (
          <>
            <DialogHeader className="shrink-0">
              <DialogTitle>{title}</DialogTitle>
              <DialogDescription>{summary}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
              {steps}
            </div>
            <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
              {actions}
            </DialogFooter>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}


export type { ConnectFlowDialogProps };
