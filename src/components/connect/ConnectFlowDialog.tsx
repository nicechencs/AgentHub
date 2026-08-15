import * as React from 'react';
import { AlertTriangle, CheckCircle2, KeyRound, RefreshCw, Wallet } from 'lucide-react';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { AgentDot } from '@/components/shared/AgentDot';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { CurrentBadge } from '@/components/shared/CurrentBadge';
import { EmptyState } from '@/components/shared/EmptyState';
import { ListRow } from '@/components/shared/ListRow';
import { Notice } from '@/components/shared/Notice';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { useAgentStatusesOptional } from '@/app/runtime';
import { useConnectionPool } from '@/app/runtime/ConnectionPoolProvider';
import { AGENT_IDS, agentDisplayName } from '@/config/agents';
import { resolveEffectiveConnection } from '@/lib/api/agent-connection';
import type { AdapterProfile } from '@/lib/api/adapter';
import { buildConnectionsGuideUrl } from '@/lib/connect-flow/connect-intent';
import {
  AGENT_ALL_INFEASIBLE_MESSAGE,
  SOURCE_ALL_INFEASIBLE_MESSAGE,
} from '@/lib/connect-flow/reuse-offer';
import type { AgentId, SwitchPreview } from '@/lib/types';
import type {
  ConnectFlowDeps,
  ConnectFlowDialogProps,
  ConnectFlowEntry,
  PlanEligibility,
  PlanFanoutController,
  SourceOption,
} from '@/lib/connect-flow/types';
import { cn } from '@/lib/utils';
import {
  beginConfirm,
  canBuildSourceOptions,
  canConfirm,
  canEnterPreview,
  canRetry,
  connectFlowEntryKey,
  connectFlowResultMessage,
  createConnectFlowState,
  currentTargetAgentId,
  describePlanPreview,
  eligibilityOf,
  excludeOwnAgentTargets,
  fanoutRequestsForAgent,
  fanoutRequestsForSource,
  findOption,
  formatConnectFlowError,
  guideTargetAgentId,
  isConnectFlowEntryStale,
  isGeneratedAdapterSource,
  isOptionSelectable,
  isPreviewInvalid,
  isProfilesReadyForEntry,
  isTargetSelectable,
  notifyConnectionChangedSafe,
  PREVIEW_SELECTION_STALE_MESSAGE,
  reduceConnectFlow,
  releaseConfirmLock,
  resolveEmptyKind,
  settleConfirm,
  shouldRevertPreviewToSelect,
  shouldShowSelectSkeleton,
  sourceAgentIdOf,
  splitSourceOptions,
  tryAcquireConfirmLock,
  type ConnectFlowState,
} from './connect-flow-state';

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
  const catalogIds = React.useMemo(
    () => (AGENT_IDS.length > 0 ? [...AGENT_IDS] : uniquePoolAgentIds(pool.accounts, pool.providers)),
    [pool.accounts, pool.providers],
  );
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
                <SelectStep
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
                <PreviewStep
                  state={state}
                  option={selectedOption}
                  previewInvalid={previewInvalid}
                  previewNative={deps.previewNative}
                  onGoImport={goImportLogin}
                />
              ) : null}

              {!entryStale && state.step === 'result' && state.result ? (
                <ResultStep result={state.result} />
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

function EffectiveSummary({
  agentId,
  label,
  authLabel,
}: {
  agentId: AgentId;
  label: string;
  authLabel: string;
}) {
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      <AgentDot agentId={agentId} size="sm" title={null} />
      <span>当前生效：{label}</span>
      <Badge variant="default">{authLabel}</Badge>
    </span>
  );
}

function FixedSourceSummary({
  entry,
  accounts,
  providers,
}: {
  entry: Extract<ConnectFlowEntry, { mode: 'for-source' }>;
  accounts: { id: string; label: string; agentId: AgentId }[];
  providers: { id: string; name: string; agentId: AgentId }[];
}) {
  const record = entry.source.kind === 'account'
    ? accounts.find((item) => item.id === entry.source.id)
    : providers.find((item) => item.id === entry.source.id);
  const label = record
    ? ('label' in record ? record.label : record.name)
    : `${entry.source.kind}:${entry.source.id}`;
  const agentId = record?.agentId;
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      {agentId ? <AgentDot agentId={agentId} size="sm" title={null} /> : null}
      <span>将「{label}」接到其他 Agent</span>
    </span>
  );
}

function SelectLoadingSkeleton() {
  return (
    <div className="space-y-2">
      <Skeleton className="h-12 w-full" />
      <Skeleton className="h-12 w-full" />
      <Skeleton className="h-12 w-full" />
    </div>
  );
}

function SelectStep({
  entry,
  state,
  options,
  eligibilities,
  targetAgentIds,
  sourceAgentId,
  emptyKind,
  poolLoading,
  profilesReady,
  onSelectSource,
  onSelectTarget,
  onRetryEligibility,
  onRetryResources,
  onGoImport,
  onGoNewKey,
  onOauthGuide,
}: {
  entry: ConnectFlowEntry;
  state: ConnectFlowState;
  options: SourceOption[];
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  targetAgentIds: AgentId[];
  sourceAgentId: AgentId | null;
  emptyKind: ReturnType<typeof resolveEmptyKind>;
  poolLoading: boolean;
  profilesReady: boolean;
  onSelectSource: (option: SourceOption) => void;
  onSelectTarget: (agentId: AgentId) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onRetryResources: () => void;
  onGoImport: () => void;
  onGoNewKey: () => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  if (emptyKind.kind === 'partial_load_error') {
    return (
      <div className="space-y-3">
        <Notice tone="danger" actionLabel="重试" onAction={onRetryResources}>
          {emptyKind.message}
        </Notice>
        {entry.mode === 'for-agent' && options.length > 0 ? (
          <SourceGroups
            options={options}
            state={state}
            eligibilities={eligibilities}
            targetAgentId={entry.targetAgentId}
            onSelectSource={onSelectSource}
            onRetryEligibility={onRetryEligibility}
            onOauthGuide={onOauthGuide}
          />
        ) : null}
      </div>
    );
  }

  if (emptyKind.kind === 'preset_invalid' || emptyKind.kind === 'preset_deleted') {
    return (
      <Notice tone="danger">{emptyKind.message}</Notice>
    );
  }

  if (shouldShowSelectSkeleton({
    profilesReady,
    poolLoading,
    optionsLength: options.length,
    targetAgentIdsLength: targetAgentIds.length,
  })) {
    return <SelectLoadingSkeleton />;
  }

  if (emptyKind.kind === 'wallet_empty') {
    return (
      <EmptyState
        icon={Wallet}
        title="还没有凭据"
        description="先到 Connections 添加登录态或 API Key，再回来连接 Agent。"
        actionLabel="去 Connections 添加"
        onAction={onGoImport}
      />
    );
  }

  return (
    <div className="space-y-4">
      {entry.mode === 'for-agent' ? (
        <SourceGroups
          options={options}
          state={state}
          eligibilities={eligibilities}
          targetAgentId={entry.targetAgentId}
          onSelectSource={onSelectSource}
          onRetryEligibility={onRetryEligibility}
          onOauthGuide={onOauthGuide}
        />
      ) : (
        <TargetGrid
          targetAgentIds={targetAgentIds}
          selected={state.selectedTargetAgentId}
          source={entry.source}
          sourceAgentId={sourceAgentId}
          eligibilities={eligibilities}
          onSelect={onSelectTarget}
          onRetryEligibility={onRetryEligibility}
          onOauthGuide={onOauthGuide}
        />
      )}

      {emptyKind.kind === 'all_infeasible' ? (
        <Notice tone="warning">
          {entry.mode === 'for-source' ? SOURCE_ALL_INFEASIBLE_MESSAGE : AGENT_ALL_INFEASIBLE_MESSAGE}
        </Notice>
      ) : null}

      <GuideActions onGoImport={onGoImport} onGoNewKey={onGoNewKey} />
    </div>
  );
}

function SourceGroups({
  options,
  state,
  eligibilities,
  targetAgentId,
  onSelectSource,
  onRetryEligibility,
  onOauthGuide,
}: {
  options: SourceOption[];
  state: ConnectFlowState;
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  targetAgentId: AgentId;
  onSelectSource: (option: SourceOption) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  const { native, cross } = splitSourceOptions(options);
  return (
    <div className="space-y-4">
      <section className="space-y-2">
        <h3 className="text-sm font-medium">本 Agent 凭据</h3>
        {native.length === 0 ? (
          <p className="text-xs text-muted">此 Agent 还没有自有凭据。</p>
        ) : native.map((item) => (
          <NativeOptionRow
            key={`${item.ref.kind}:${item.ref.id}`}
            option={item}
            active={state.selectedSource?.kind === item.ref.kind && state.selectedSource.id === item.ref.id}
            onSelect={onSelectSource}
          />
        ))}
      </section>
      <section className="space-y-2">
        <h3 className="text-sm font-medium">其他服务凭据</h3>
        {cross.length === 0 ? (
          <p className="text-xs text-muted">钱包里暂无其他服务凭据。</p>
        ) : cross.map((item) => (
          <CrossOptionRow
            key={`${item.ref.kind}:${item.ref.id}`}
            option={item}
            eligibility={eligibilityOf(eligibilities, item.ref, targetAgentId)}
            active={state.selectedSource?.kind === item.ref.kind && state.selectedSource.id === item.ref.id}
            onSelect={onSelectSource}
            onRetry={() => onRetryEligibility({ source: item.ref, targetAgentId })}
            onOauthGuide={() => onOauthGuide(item.agentId)}
          />
        ))}
      </section>
    </div>
  );
}

function NativeOptionRow({
  option,
  active,
  onSelect,
}: {
  option: SourceOption;
  active: boolean;
  onSelect: (option: SourceOption) => void;
}) {
  const disabled = option.state.kind === 'current' || option.state.kind === 'blocked_native';
  return (
    <ListRow
      active={active}
      className={cn('px-3 py-2', disabled && 'opacity-60')}
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled}
      onClick={() => {
        if (!disabled) onSelect(option);
      }}
      onKeyDown={(event) => {
        if (disabled) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(option);
        }
      }}
    >
      <div className="flex min-w-0 items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{option.label}</p>
          {option.sublabel ? <p className="truncate text-xs text-muted">{option.sublabel}</p> : null}
          {option.viaAdapter ? (
            <p className="mt-0.5 text-xs text-secondary">
              经兼容路由 · 来源 {option.viaAdapter.sourceLabel}
            </p>
          ) : null}
          {option.state.kind === 'blocked_native' ? (
            <p className="mt-0.5 text-xs text-warning">{option.state.reason}</p>
          ) : null}
        </div>
        {option.state.kind === 'current' ? (
          <span className="flex items-center gap-1">
            <CurrentBadge />
            <span className="text-xs text-secondary">当前使用</span>
          </span>
        ) : null}
      </div>
    </ListRow>
  );
}

function CrossOptionRow({
  option,
  eligibility,
  active,
  onSelect,
  onRetry,
  onOauthGuide,
}: {
  option: SourceOption;
  eligibility: PlanEligibility | undefined;
  active: boolean;
  onSelect: (option: SourceOption) => void;
  onRetry: () => void;
  onOauthGuide: () => void;
}) {
  const selectable = isOptionSelectable(option, eligibility);
  const disabled = !selectable;
  return (
    <ListRow
      active={active && selectable}
      className={cn('px-3 py-2', disabled && eligibility?.kind !== 'loading' && 'opacity-60')}
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled}
      onClick={() => {
        if (selectable) onSelect(option);
      }}
      onKeyDown={(event) => {
        if (!selectable) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(option);
        }
      }}
    >
      <div className="flex min-w-0 items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-1.5 truncate text-sm font-medium">
            <AgentDot agentId={option.agentId} size="sm" title={null} />
            {option.label}
          </p>
          {option.sublabel ? <p className="truncate text-xs text-muted">{option.sublabel}</p> : null}
          <EligibilityBody
            eligibility={eligibility}
            onRetry={onRetry}
            onOauthGuide={onOauthGuide}
          />
        </div>
      </div>
    </ListRow>
  );
}

function TargetGrid({
  targetAgentIds,
  selected,
  source,
  sourceAgentId,
  eligibilities,
  onSelect,
  onRetryEligibility,
  onOauthGuide,
}: {
  targetAgentIds: AgentId[];
  selected: AgentId | null;
  source: SourceOption['ref'];
  sourceAgentId: AgentId | null;
  eligibilities: ReadonlyMap<string, PlanEligibility>;
  onSelect: (agentId: AgentId) => void;
  onRetryEligibility: (request: { source: SourceOption['ref']; targetAgentId: AgentId }) => void;
  onOauthGuide: (agentId: AgentId) => void;
}) {
  if (targetAgentIds.length === 0) {
    return <p className="text-sm text-muted">没有可选择的其他 Agent。</p>;
  }
  return (
    <div className="grid grid-cols-2 gap-2">
      {targetAgentIds.map((agentId) => {
        const eligibility = eligibilityOf(eligibilities, source, agentId);
        const selectable = isTargetSelectable(eligibility);
        const active = selected === agentId;
        return (
          <div
            key={agentId}
            role="button"
            tabIndex={selectable ? 0 : -1}
            aria-disabled={!selectable}
            onClick={() => {
              if (selectable) onSelect(agentId);
            }}
            onKeyDown={(event) => {
              if (!selectable) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(agentId);
              }
            }}
            className={cn(
              'rounded-card border border-border bg-panel p-3 text-left transition-colors',
              active && selectable && 'border-border-strong bg-active',
              !selectable && 'opacity-60',
              selectable && 'hover:bg-hover/50',
            )}
          >
            <div className="flex items-center gap-2">
              <AgentLogo agentId={agentId} size="sm" />
              <span className="text-sm font-medium">{agentDisplayName(agentId)}</span>
            </div>
            <EligibilityBody
              eligibility={eligibility}
              onRetry={() => onRetryEligibility({ source, targetAgentId: agentId })}
              onOauthGuide={() => onOauthGuide(sourceAgentId ?? agentId)}
            />
          </div>
        );
      })}
    </div>
  );
}

function EligibilityBody({
  eligibility,
  onRetry,
  onOauthGuide,
}: {
  eligibility: PlanEligibility | undefined;
  onRetry: () => void;
  onOauthGuide: () => void;
}) {
  if (!eligibility || eligibility.kind === 'loading') {
    return <Skeleton className="mt-2 h-3 w-28" />;
  }
  if (eligibility.kind === 'blocked_oauth') {
    return (
      <p className="mt-1 text-xs text-warning">
        {eligibility.message}{' '}
        <button
          type="button"
          className="underline"
          onClick={(event) => {
            event.stopPropagation();
            onOauthGuide();
          }}
        >
          去 Connections 完成登录
        </button>
      </p>
    );
  }
  if (eligibility.kind === 'error') {
    return (
      <p className="mt-1 flex items-center gap-2 text-xs text-danger">
        <span className="min-w-0 flex-1">{eligibility.message}</span>
        <Button
          size="sm"
          variant="outline"
          onClick={(event) => {
            event.stopPropagation();
            onRetry();
          }}
        >
          <RefreshCw className="h-3 w-3" /> 重试
        </Button>
      </p>
    );
  }
  if (eligibility.canApply) {
    return <p className="mt-1 text-xs text-secondary">{eligibility.routeSummary}</p>;
  }
  return <p className="mt-1 text-xs text-warning">{eligibility.reason ?? eligibility.routeSummary}</p>;
}

function GuideActions({
  onGoImport,
  onGoNewKey,
}: {
  onGoImport: () => void;
  onGoNewKey: () => void;
}) {
  return (
    <section className="space-y-2 rounded-btn border border-border bg-subtle/60 p-3">
      <p className="text-xs font-medium text-secondary">其他连接方式</p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={onGoImport}>
          <Wallet className="h-3.5 w-3.5" />
          导入已有登录态
        </Button>
        <Button size="sm" variant="outline" onClick={onGoNewKey}>
          <KeyRound className="h-3.5 w-3.5" />
          新 API Key
        </Button>
      </div>
      <p className="text-xs text-muted">
        未登录请先在对应官方 CLI 完成登录，再返回导入。新 API Key 会跳到 Connections 供应商列表。
      </p>
    </section>
  );
}

function SwitchPreviewFacts({ preview }: { preview: SwitchPreview }) {
  return (
    <div className="flex flex-col gap-2.5 text-sm">
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">{preview.backfillSummary}</span>
      </div>
      <div className="flex items-start gap-2">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
        <span className="text-secondary">
          切换前备份到 <code className="font-mono text-xs">{preview.backupPath}</code>
        </span>
      </div>
      {preview.processWarning ? (
        <div className="flex items-start gap-2">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
          <span className="text-warning">{preview.processWarning}</span>
        </div>
      ) : null}
    </div>
  );
}

function SwitchNativePreview({
  option,
  fallbackLabel,
  lastError,
  previewNative,
}: {
  option: SourceOption | null;
  fallbackLabel?: string;
  lastError: string | null;
  previewNative?: ConnectFlowDeps['previewNative'];
}) {
  const label = option?.label ?? fallbackLabel;
  const shouldFetch = Boolean(option?.ref.kind === 'provider' && previewNative);
  const [phase, setPhase] = React.useState<'idle' | 'loading' | 'ready' | 'error'>(
    shouldFetch ? 'loading' : 'idle',
  );
  const [preview, setPreview] = React.useState<SwitchPreview | null>(null);
  const [previewError, setPreviewError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!shouldFetch || !option || !previewNative) {
      setPhase('idle');
      setPreview(null);
      setPreviewError(null);
      return;
    }
    let cancelled = false;
    setPhase('loading');
    setPreview(null);
    setPreviewError(null);
    void previewNative(option).then(
      (result) => {
        if (cancelled) return;
        setPreview(result);
        setPhase('ready');
      },
      (error: unknown) => {
        if (cancelled) return;
        setPreviewError(formatConnectFlowError(error));
        setPhase('error');
      },
    );
    return () => {
      cancelled = true;
    };
  }, [shouldFetch, option, previewNative]);

  const nativeHint = (
    <p className="text-xs text-secondary">将走本 Agent 既有切换，不会创建跨服务适配。</p>
  );

  if (phase === 'loading') {
    return (
      <div className="space-y-2 text-sm">
        <p>切换到「{label}」？</p>
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <p className="text-xs text-muted">正在预览…</p>
        {nativeHint}
      </div>
    );
  }

  if (phase === 'error') {
    return (
      <div className="space-y-2 text-sm">
        <p>切换到「{label}」？</p>
        <Notice tone="danger">{previewError}</Notice>
        {nativeHint}
      </div>
    );
  }

  return (
    <div className="space-y-2 text-sm">
      <p>切换到「{label}」？</p>
      {preview ? <SwitchPreviewFacts preview={preview} /> : null}
      {nativeHint}
      {lastError ? <Notice tone="danger">{lastError}</Notice> : null}
    </div>
  );
}

function PreviewStep({
  state,
  option,
  previewInvalid,
  previewNative,
  onGoImport,
}: {
  state: ConnectFlowState;
  option: SourceOption | null;
  previewInvalid: boolean;
  previewNative?: ConnectFlowDeps['previewNative'];
  onGoImport: () => void;
}) {
  if (previewInvalid) {
    return <Notice tone="warning">{PREVIEW_SELECTION_STALE_MESSAGE}</Notice>;
  }

  if (state.previewKind === 'switch') {
    return (
      <SwitchNativePreview
        option={option}
        fallbackLabel={state.selectedSource?.id}
        lastError={state.lastError}
        previewNative={previewNative}
      />
    );
  }

  if (!state.boundPlan) {
    return <Notice tone="warning">没有可应用的预览，请返回重新选择。</Notice>;
  }

  const view = describePlanPreview(state.boundPlan.plan);
  return (
    <div className="space-y-3 text-sm">
      <div>
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="font-medium">{view.routeLabel}</h3>
          <Badge variant="success">可应用</Badge>
        </div>
        {view.reason ? <p className="mt-1 text-secondary">{view.reason}</p> : null}
      </div>
      <section>
        <h4 className="text-xs font-medium text-secondary">将写入的配置</h4>
        {view.writes.length > 0 ? (
          <ul className="mt-1 list-disc space-y-0.5 pl-5 text-secondary">
            {view.writes.map((line) => <li key={line}>{line}</li>)}
          </ul>
        ) : (
          <p className="mt-1 text-secondary">无需写入配置。</p>
        )}
      </section>
      <section className="space-y-0.5 text-secondary">
        <p>服务影响：{view.serviceImpact}</p>
        {view.startsBridge ? <p>将启动本机桥。</p> : <p>不启动本机桥。</p>}
        {view.portNotes.map((line) => <p key={line}>端口：{line}</p>)}
        {view.modelMappings.length > 0 ? (
          <div>
            <p>模型映射：</p>
            <ul className="list-disc pl-5">
              {view.modelMappings.map((line) => <li key={line}>{line}</li>)}
            </ul>
          </div>
        ) : null}
      </section>
      {view.limitations.length > 0 ? (
        <section>
          <h4 className="text-xs font-medium text-secondary">限制</h4>
          <ul className="mt-1 list-disc pl-5 text-secondary">
            {view.limitations.map((line) => <li key={line}>{line}</li>)}
          </ul>
        </section>
      ) : null}
      {state.lastError ? <Notice tone="danger">{state.lastError}</Notice> : null}
      <p className="text-xs text-muted">
        若来源尚未登录，请先在官方 CLI 完成登录再{' '}
        <button type="button" className="underline" onClick={onGoImport}>去 Connections 导入</button>
        。
      </p>
    </div>
  );
}

function ResultStep({ result }: { result: NonNullable<ConnectFlowState['result']> }) {
  const message = connectFlowResultMessage(result);
  const tone = result.kind === 'failed' ? 'danger' : result.refreshFailed ? 'warning' : 'success';
  return (
    <Notice tone={tone}>
      <p className="text-sm font-medium text-primary">{message}</p>
      {result.kind === 'applied' && result.isCurrent && !result.refreshFailed ? (
        <p className="mt-1">目标 Agent 已使用新的连接。</p>
      ) : null}
      {result.kind === 'failed' ? (
        <p className="mt-1">选择与预览仍保留，可直接重试。</p>
      ) : null}
    </Notice>
  );
}

export type { ConnectFlowDialogProps };
