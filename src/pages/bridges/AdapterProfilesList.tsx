import { useState } from 'react';
import { ArrowRight, Boxes } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { ListRow } from '@/components/shared/ListRow';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TicketSurfaceGroupView } from '@/lib/backend/contracts/ticket';
import { AdapterErrorLines } from './adapter-components';
import { AdapterProfileDetailDialog } from './AdapterProfileDetailDialog';
import {
  adapterBridgeHostPort,
  adapterFailurePresentation,
} from './adapter-model';
import {
  isAlternateRouteRule,
  listLocalRouteSurfacesFromConfig,
  type CreateRouteTarget,
} from './create-route-flow';
import {
  adapterProfilePrimaryAction,
  adapterProfileRecoveryGuide,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
  resolveAdapterProfileSource,
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
  /** Per-profile mutation errors (start/stop/autostart/unbind). */
  errors: Record<string, unknown>;
  busyProfileIds: Record<string, boolean>;
  removingProfileId: string | null;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onSetAutoStart?: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove?: (profile: AdapterProfile) => void;
  onApplyRoute?: (profile: AdapterProfile, agents: readonly CreateRouteTarget[]) => void;
  onShowDetail?: (profile: AdapterProfile) => void;
  onRetry: () => void;
  hiddenTargetIds?: ReadonlySet<string>;
  surfaceGroups?: readonly TicketSurfaceGroupView[];
};

/**
 * Local-bridge runtimes as a compact service list. Row surface: single-layer
 * health, source → target, endpoint copy, and the state-matched primary action.
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
  onSetAutoStart,
  onRequestRemove,
  onApplyRoute,
  onShowDetail,
  onRetry,
  hiddenTargetIds,
  surfaceGroups = [],
}: AdapterProfilesListProps) {
  const { t } = useI18n();
  const [collapsedIds, setCollapsedIds] = useState<Record<string, boolean>>({});
  const toggleDetail = (profile: AdapterProfile) => {
    setCollapsedIds((current) => ({ ...current, [profile.id]: !current[profile.id] }));
    onShowDetail?.(profile);
  };
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
      {profiles.map((profile) => (
        <AdapterProfileRow
          key={profile.id}
          profile={profile}
          bridgeStatus={profile.route === 'local_bridge' ? bridgeStatuses[profile.id] : undefined}
          statusUnavailable={Boolean(statusErrors[profile.id])}
          entries={entries}
          busy={busyProfileIds[profile.id] === true || removingProfileId === profile.id}
          error={errors[profile.id]}
          onStartBridge={onStartBridge}
          onRequestStopBridge={onRequestStopBridge}
          onSetAutoStart={onSetAutoStart}
          onRequestRemove={onRequestRemove}
          onApplyRoute={onApplyRoute}
          onToggleDetail={() => toggleDetail(profile)}
          detailExpanded={collapsedIds[profile.id] !== true}
          surfaceGroups={surfaceGroups}
          targetHidden={hiddenTargetIds?.has(profile.targetAgentId) === true}
        />
      ))}
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
  onSetAutoStart,
  onRequestRemove,
  onApplyRoute,
  onToggleDetail,
  detailExpanded,
  surfaceGroups,
  targetHidden,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  busy: boolean;
  error: unknown;
  onStartBridge: (profile: AdapterProfile) => void;
  onRequestStopBridge: (profile: AdapterProfile) => void;
  onSetAutoStart?: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove?: (profile: AdapterProfile) => void;
  onApplyRoute?: (profile: AdapterProfile, agents: readonly CreateRouteTarget[]) => void;
  onToggleDetail: () => void;
  detailExpanded: boolean;
  surfaceGroups: readonly TicketSurfaceGroupView[];
  targetHidden: boolean;
}) {
  const { t } = useI18n();
  const source = resolveAdapterProfileSource(profile, entries);
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  }, t);
  const endpointParts = profile.route === 'local_bridge'
    ? adapterBridgeHostPort(profile, bridgeStatus)
    : null;
  const sourceEntry = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  const surfaces = profile.route === 'local_bridge'
    ? listLocalRouteSurfacesFromConfig(sourceEntry?.provider?.configText, {
        targetAgentId: profile.targetAgentId,
        ruleId: profile.ruleId,
      })
    : [];
  const action = adapterProfilePrimaryAction({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    lastErrorCode: profile.lastErrorCode,
    statusUnavailable,
  }, t);
  const transitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const failure = error ? adapterFailurePresentation(error, t('routes.mutationFailure'), t) : null;

  return (
    <ListRow className="p-3">
      <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:gap-4">
        <div className="w-40 shrink-0">
          {runtimeStatus ? <StatusLine view={runtimeStatus} emphasis /> : null}
        </div>
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
            {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
            <span className="truncate">{source.title}</span>
            {isAlternateRouteRule(profile.ruleId) ? (
              <span className="rounded-full bg-muted px-1.5 py-0.5 text-meta font-medium text-secondary">
                {t('routes.create.alternate')}
              </span>
            ) : null}
            <ArrowRight className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden />
          </div>
          {surfaces.length > 0 ? (
            <ul className="space-y-0.5">
              {surfaces.map((surface) => (
                <li key={surface.target} className="flex min-w-0 flex-wrap items-center gap-1.5">
                  <span className="w-12 shrink-0 text-xs text-muted">
                    {surface.target === 'claude'
                      ? t('routes.create.target.claude')
                      : surface.target === 'codex'
                        ? t('routes.create.target.codex')
                        : t('routes.create.target.grok')}
                  </span>
                  <CopyableRouteEndpointUrl
                    path={surface.path}
                    port={endpointParts?.port}
                    host={endpointParts?.host}
                    endpointId={surface.endpointId}
                    className="text-xs"
                  />
                </li>
              ))}
            </ul>
          ) : null}
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            {endpointParts && !endpointParts.port ? (
              <span className="text-xs text-muted">{t('routes.pendingPort')}</span>
            ) : null}
            {source.missing ? (
              <span className="text-xs text-warning">{t('routes.sourceDeleted')}</span>
            ) : null}
            {targetHidden ? (
              <span className="text-xs text-muted">{t('routes.targetHidden')}</span>
            ) : null}
            {isAlternateRouteRule(profile.ruleId) ? (
              <span className="text-xs text-muted">{t('routes.create.alternate')}</span>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {action ? (
            <Button
              variant="outline"
              size="sm"
              disabled={busy || transitioning || (targetHidden && action.kind !== 'stop')}
              title={
                targetHidden && action.kind !== 'stop'
                  ? t('routes.targetHiddenHint')
                  : undefined
              }
              onClick={() => (action.kind === 'stop' ? onRequestStopBridge(profile) : onStartBridge(profile))}
            >
              {busy ? t('routes.busy') : action.label}
            </Button>
          ) : null}
          <Button
            variant="outline"
            size="sm"
            disabled={busy || targetHidden || surfaces.length === 0}
            onClick={() => onApplyRoute?.(profile, surfaces.map((surface) => surface.target))}
          >
            {t('routes.quickApply.action')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            aria-expanded={detailExpanded}
            onClick={onToggleDetail}
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
      {detailExpanded ? (
        <AdapterProfileDetailDialog
          profile={profile}
          bridgeStatus={bridgeStatus}
          statusUnavailable={statusUnavailable}
          entries={entries}
          surfaceGroups={surfaceGroups}
          busy={busy}
          error={error}
          onClose={onToggleDetail}
          onSetAutoStart={onSetAutoStart ?? (() => undefined)}
          onRequestRemove={onRequestRemove ?? (() => undefined)}
          onApplyRoute={onApplyRoute}
          targetHidden={targetHidden}
        />
      ) : null}
    </ListRow>
  );
}

function StatusLine({ view, emphasis = false }: { view: AdapterStatusView; emphasis?: boolean }) {
  return (
    <span className={emphasis ? 'flex items-center gap-1.5 text-sm' : 'flex items-center gap-1.5 text-xs'}>
      <StatusPin
        tone={view.tone}
        size="md"
        className={view.pulse ? 'animate-pulse' : undefined}
      />
      <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
    </span>
  );
}
