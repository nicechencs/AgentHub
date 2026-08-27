import { useCallback, useMemo, type ReactNode } from 'react';
import { ArrowRight, Boxes, Pencil } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SortHandle } from '@/components/shared/SortHandle';
import { StatusPin } from '@/components/shared/StatusPin';
import { ListRow } from '@/components/shared/ListRow';
import { useSortableDrag } from '@/components/shared/use-sortable-drag';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Hint, Tip } from '@/components/ui/tooltip';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from './adapter-components';
import {
  adapterBridgeHostPort,
  adapterFailurePresentation,
} from './adapter-model';
import { routeDetailTargetLabel } from './adapter-route-detail-model';
import {
  buildRouteGraph,
  routeGraphSupportedAgents,
  type RouteGraphView,
} from './route-graph-model';
import { localAddressCopyForTarget } from './route-endpoint-copy';
import {
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
  isLocalBridgeCardActive,
  type AdapterStatusView,
} from './adapter-view-model';

export type AdapterProfilesListProps = {
  profiles: AdapterProfile[];
  bridgeStatuses: Record<string, AdapterBridgeRuntimeStatus>;
  /** Per-profile bridge status *read* failures — shown as 状态不可用, not as a bridge fault. */
  statusErrors: Record<string, unknown>;
  /** Full (unfiltered) connection pool for source name resolution. */
  entries: ConnectionEntry[];
  loading: boolean;
  loadError: unknown;
  /** Per-profile mutation errors (start/stop/unbind). */
  errors: Record<string, unknown>;
  busyProfileIds: Record<string, boolean>;
  removingProfileId: string | null;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  /** Opens the dedicated client-config write dialog for one route. */
  onRequestWrite?: (profile: AdapterProfile, graph: RouteGraphView) => void;
  onRequestEdit?: (profile: AdapterProfile) => void;
  onShowDetail?: (profile: AdapterProfile) => void;
  /** Profile currently in the inspect pane (detail / edit / write). */
  activeProfileId?: string | null;
  onMove?: (fromId: string, toId: string) => void;
  onRetry: () => void;
  hiddenTargetIds?: ReadonlySet<string>;
  siblingProfiles?: readonly AdapterProfile[];
};

/**
 * Local-bridge runtimes as a compact service list. Row surface: single-layer
 * health, upstream → loopback flow, the clients this route serves, and the
 * state-matched primary action.
 */
export function AdapterProfilesList({
  profiles,
  bridgeStatuses,
  statusErrors,
  entries,
  loading,
  loadError,
  errors,
  busyProfileIds,
  removingProfileId,
  onStartBridge,
  onRequestStopBridge,
  onRequestWrite,
  onRequestEdit,
  onShowDetail,
  activeProfileId,
  onMove,
  onRetry,
  hiddenTargetIds,
  siblingProfiles,
}: AdapterProfilesListProps) {
  const { t } = useI18n();
  const liveIds = useMemo(() => profiles.map((profile) => profile.id), [profiles]);
  const canReorder = Boolean(onMove) && liveIds.length > 1;
  const { onDragStartId, rowProps } = useSortableDrag((fromId, toId) => onMove?.(fromId, toId));
  const moveNeighbor = useCallback((id: string, direction: -1 | 1) => {
    const index = liveIds.indexOf(id);
    const next = liveIds[index + direction];
    if (!next || !onMove) return;
    onMove(id, next);
  }, [liveIds, onMove]);
  if (loading) {
    return (
      <div className="space-y-2" aria-live="polite">
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-16 w-full" />
      </div>
    );
  }
  if (loadError) {
    return (
      <ErrorState
        error={loadError}
        title={t('routes.loadError')}
        onRetry={onRetry}
      />
    );
  }
  if (profiles.length === 0) {
    return (
      <EmptyState
        icon={Boxes}
        title={t('routes.empty.title')}
        description={t('routes.empty.description')}
      />
    );
  }
  return (
    <div className="space-y-2">
      {profiles.map((profile) => {
        const sortable = rowProps(profile.id);
        return (
          <div key={profile.id} {...sortable}>
            <AdapterProfileRow
              profile={profile}
              bridgeStatus={profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined}
              statusUnavailable={Boolean(statusErrors[profile.id])}
              entries={entries}
              busy={busyProfileIds[profile.id] === true || removingProfileId === profile.id}
              error={errors[profile.id]}
              onStartBridge={onStartBridge}
              onRequestStopBridge={onRequestStopBridge}
              onRequestWrite={onRequestWrite}
              onRequestEdit={onRequestEdit}
              onShowDetail={onShowDetail}
              active={isLocalBridgeCardActive(profile, activeProfileId, siblingProfiles ?? profiles)}
              targetHidden={hiddenTargetIds?.has(profile.targetAgentId) === true}
              siblingProfiles={siblingProfiles ?? profiles}
              sortHandle={canReorder ? (
                <SortHandle
                  id={profile.id}
                  onDragStartId={onDragStartId}
                  onMoveNeighbor={moveNeighbor}
                />
              ) : null}
            />
          </div>
        );
      })}
    </div>
  );
}

function AdapterProfileRow({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  busy,
  error,
  onStartBridge,
  onRequestStopBridge,
  onRequestWrite,
  onRequestEdit,
  onShowDetail,
  active,
  targetHidden,
  siblingProfiles,
  sortHandle,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  busy: boolean;
  error: unknown;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onRequestWrite?: (profile: AdapterProfile, graph: RouteGraphView) => void;
  onRequestEdit?: (profile: AdapterProfile) => void;
  onShowDetail?: (profile: AdapterProfile) => void;
  active: boolean;
  targetHidden: boolean;
  siblingProfiles: readonly AdapterProfile[];
  sortHandle?: ReactNode;
}) {
  const { t } = useI18n();
  const endpointParts = profile.route === 'local_bridge'
    ? adapterBridgeHostPort(profile, bridgeStatus)
    : null;
  const graph = buildRouteGraph({
    profile,
    entries,
    siblingProfiles,
    host: endpointParts?.host,
    port: endpointParts?.port,
  });
  const source = graph.source;
  const supportedAgents = routeGraphSupportedAgents(graph.rows);
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  }, t);
  const action = adapterProfilePrimaryAction({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    lastErrorCode: profile.lastErrorCode,
    statusUnavailable,
  }, t);
  const transitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const failure = error ? adapterFailurePresentation(error, t('routes.mutationFailure'), t) : null;
  const localLabel = graph.local.origin || t('routes.pendingPort');

  return (
    <ListRow className="p-3" active={active}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        {sortHandle}
        <div className="w-40 shrink-0">
          {runtimeStatus ? <StatusLine view={runtimeStatus} emphasis /> : null}
        </div>
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
            {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
            <span className="truncate">{source.title}</span>
          </div>
          <div className="flex min-w-0 flex-wrap items-center gap-1.5 font-mono text-xs text-secondary">
            <Tip label={source.baseUrl || undefined} className="truncate">
              {source.baseUrl || t('routes.graph.upstreamUnknown')}
            </Tip>
            <ArrowRight className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden />
            <span className="truncate">{localLabel}</span>
          </div>
          {supportedAgents.length > 0 ? (
            <div className="flex min-w-0 flex-wrap items-center gap-1.5">
              <span className="text-xs text-muted">{t('routes.supports')}</span>
              {supportedAgents.map((agent) => {
                const local = localAddressCopyForTarget(agent);
                return (
                  <span
                    key={agent}
                    className="inline-flex items-center gap-1 rounded-full bg-muted/40 px-1.5 py-0.5 text-meta font-medium text-secondary"
                    title={`${local.path} · ${t(local.copyKey)}`}
                  >
                    <AgentDot agentId={agent} size="sm" title={null} />
                    {routeDetailTargetLabel(agent, t)}
                    <span className="font-mono font-normal text-muted">{local.path}</span>
                  </span>
                );
              })}
            </div>
          ) : null}
          {supportedAgents.length > 0 ? (
            <p className="text-meta text-muted">{t('routes.endpoint.modelsLine')}</p>
          ) : null}
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            {source.missing ? (
              <span className="text-xs text-warning">{t('routes.sourceDeleted')}</span>
            ) : null}
            {targetHidden ? (
              <span className="text-xs text-muted">{t('routes.targetHidden')}</span>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {action ? (
            <Hint
              label={
                targetHidden && action.kind !== 'stop'
                  ? t('routes.targetHiddenHint')
                  : undefined
              }
            >
              <Button
                variant={action.kind === 'stop' ? 'dangerOutline' : 'outline'}
                size="sm"
                disabled={busy || transitioning || (targetHidden && action.kind !== 'stop')}
                onClick={() => (action.kind === 'stop' ? onRequestStopBridge(profile) : onStartBridge(profile))}
              >
                {busy ? t('routes.busy') : action.label}
              </Button>
            </Hint>
          ) : null}
          {profile.route === 'local_bridge' ? (
            <Button
              variant="outline"
              size="sm"
              disabled={busy || targetHidden || graph.rows.length === 0}
              onClick={() => onRequestWrite?.(profile, graph)}
            >
              {t('routes.write.action')}
            </Button>
          ) : null}
          {onRequestEdit ? (
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => onRequestEdit(profile)}
            >
              <Pencil className="h-3.5 w-3.5" /> {t('routes.edit.action')}
            </Button>
          ) : null}
          <Button
            variant="outline"
            size="sm"
            onClick={() => onShowDetail?.(profile)}
          >
            {t('routes.detail')}
          </Button>
        </div>
      </div>
      {recovery ? (
        <p className="mt-2 text-xs text-warning" role="status">
          {`${recovery.summary} ${t('routes.recovery.openDetail')}`}
        </p>
      ) : null}
      {failure ? (
        <div className="mt-2 space-y-1" role="alert">
          <AdapterErrorLines error={error} fallback={t('routes.mutationFailure')} />
          <p className="text-xs text-secondary">{failure.hint}</p>
        </div>
      ) : null}
    </ListRow>
  );
}

function StatusLine({ view, emphasis = false }: { view: AdapterStatusView; emphasis?: boolean }) {
  return (
    <span className={cn('flex items-center gap-1.5', emphasis ? 'text-sm' : 'text-xs')}>
      <StatusPin
        tone={view.tone}
        size="md"
        className={view.pulse ? 'animate-pulse' : undefined}
      />
      <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
    </span>
  );
}
