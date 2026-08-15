import * as React from 'react';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
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
import { resolveEffectiveConnection } from '@/lib/api/agent-connection';
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
  sourceAgentIdOf,
  tryAcquireConfirmLock,
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
}: ConnectFlowDialogProps) {
  const open = entry !== null;
  const key = connectFlowEntryKey(entry);
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
    });
  }, [entry, deps, pool.accounts, pool.providers, profiles, optionsReady, statuses]);

  const sourceAgentId = entry
    ? sourceAgentIdOf(entry, pool.accounts, pool.providers)
    : null;
  // 引用必须稳定：这些数组直接进 effect 依赖，逐渲染新建会反复触发 fanout.start
  const hiddenSet = React.useMemo(() => hiddenAgentIdSet(statuses ?? []), [statuses]);
  const catalogIds = React.useMemo(() => {
    const ids = AGENT_IDS.length > 0 ? [...AGENT_IDS] : uniquePoolAgentIds(pool.accounts, pool.providers);
    return ids.filter((id) => !hiddenSet.has(id));
  }, [pool.accounts, pool.providers, hiddenSet]);
  const targetAgentIds = React.useMemo(
    () => (entry?.mode === 'for-source' ? excludeOwnAgentTargets(catalogIds, sourceAgentId) : []),
    [entry, catalogIds, sourceAgentId],
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
      : fanoutRequestsForSource(entry.source, targetAgentIds);
  }, [entry, options, targetAgentIds, optionsReady, generatedSourceBlocked]);

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
      })
    : { kind: 'none' as const };

  const currentAccount = targetId
    ? pool.accounts.find((item) => item.agentId === targetId && item.isCurrent)
    : undefined;
  const currentProvider = targetId
    ? pool.providers.find((item) => item.agentId === targetId && item.isCurrent)
    : undefined;
  const effective = resolveEffectiveConnection(currentAccount, currentProvider);

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

  const goNewApiKey = React.useCallback(() => {
    if (!guideAgent) {
      navigateTo('/connections?mode=providers');
      return;
    }
    navigateTo(buildConnectionsGuideUrl({
      agentId: guideAgent,
      intent: 'add-key',
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
  }, [state, selectedOption, previewInvalid]);

  const retryResources = React.useCallback(() => {
    void pool.reload();
    void loadProfiles(key);
  }, [pool, loadProfiles, key]);

  const title = !entry
    ? '连接'
    : entry.mode === 'for-agent'
      ? `连接 ${agentDisplayName(entry.targetAgentId)}`
      : '接到…';

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
              <DialogDescription>
                {entry.mode === 'for-agent' ? (
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
                )}
              </DialogDescription>
            </DialogHeader>

            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
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
                  })}
                  onRetryEligibility={(request) => fanout?.retry(request)}
                  onRetryResources={retryResources}
                  onGoImport={goImportLogin}
                  onGoNewKey={goNewApiKey}
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
                />
              ) : null}

              {!entryStale && state.step === 'result' && state.result ? (
                <ConnectFlowResultStep result={state.result} />
              ) : null}
            </div>

            <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
              {entryStale || state.step === 'select' ? (
                <>
                  <Button variant="secondary" disabled={busy} onClick={requestClose}>取消</Button>
                  {!entryStale ? (
                    <Button
                      disabled={busy || !canEnterPreview(state, selectedOption, selectedEligibility)}
                      onClick={() => dispatch({
                        type: 'enter_preview',
                        option: selectedOption,
                        eligibility: selectedEligibility,
                      })}
                    >
                      下一步
                    </Button>
                  ) : null}
                </>
              ) : null}
              {!entryStale && state.step === 'preview' ? (
                <>
                  <Button variant="secondary" disabled={busy} onClick={() => dispatch({ type: 'back_to_select' })}>
                    返回
                  </Button>
                  <Button disabled={!canConfirm(state) || previewInvalid} onClick={handleConfirm}>
                    {busy
                      ? (state.busy === 'switching' ? '切换中…' : '应用中…')
                      : (state.previewKind === 'switch' ? '确认切换' : '确认应用')}
                  </Button>
                </>
              ) : null}
              {!entryStale && state.step === 'result' ? (
                <>
                  {canRetry(state) ? (
                    <Button variant="secondary" onClick={() => dispatch({ type: 'retry_from_result' })}>
                      重试
                    </Button>
                  ) : null}
                  <Button onClick={requestClose}>关闭</Button>
                </>
              ) : null}
            </DialogFooter>
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}


export type { ConnectFlowDialogProps };
