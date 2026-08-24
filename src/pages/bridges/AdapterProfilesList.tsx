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
import { routeEndpointIdForBinding, routeEndpointPathForBinding } from '@/lib/route-endpoints';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { AdapterErrorLines } from './adapter-components';
import {
  adapterBridgeHostPort,
  adapterFailurePresentation,
} from './adapter-model';
import { isAlternateRouteRule } from './create-route-flow';
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
  onShowDetail: (profile: AdapterProfile) => void;
  onRetry: () => void;
  hiddenTargetIds?: ReadonlySet<string>;
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
  onShowDetail,
  onRetry,
  hiddenTargetIds,
}: AdapterProfilesListProps) {
  const { t } = useI18n();
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
          onShowDetail={onShowDetail}
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
  onShowDetail,
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
  onShowDetail: (profile: AdapterProfile) => void;
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
  const endpointPath = profile.route === 'local_bridge'
    ? routeEndpointPathForBinding({
        agentId: profile.targetAgentId,
        ruleId: profile.ruleId,
      })
    : null;
  const endpointId = profile.route === 'local_bridge'
    ? routeEndpointIdForBinding({
        agentId: profile.targetAgentId,
        ruleId: profile.ruleId,
      })
    : null;
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
            {endpointPath && endpointId ? (
              <CopyableRouteEndpointUrl
                path={endpointPath}
                port={endpointParts?.port}
                host={endpointParts?.host}
                endpointId={endpointId}
                className="text-xs"
              />
            ) : null}
          </div>
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
          <Button variant="ghost" size="sm" onClick={() => onShowDetail(profile)}>
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
