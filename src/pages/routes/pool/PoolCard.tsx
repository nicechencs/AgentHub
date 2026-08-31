import { agentDisplayName } from '@/config/agents';
import { AgentDot } from '@/components/shared/AgentDot';
import { ListRow, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { StatusPin } from '@/components/shared/StatusPin';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
import type { TranslateFn } from '@/lib/i18n';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from '@/pages/bridges/adapter-components';
import { adapterFailurePresentation } from '@/pages/bridges/adapter-model';
import {
  adapterProfilePrimaryAction,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
  type AdapterStatusView,
} from '@/pages/bridges/adapter-view-model';
import {
  defaultPoolEntryUrl,
  routePoolMemberLabels,
  routePoolSurfaceLabel,
  type PoolWorkbenchRow,
} from '@/pages/bridges/route-pool-view-model';

function memberAvailabilityLabel(
  availability: string | undefined,
  enabled: boolean,
  t: TranslateFn,
): string {
  if (!enabled || availability === 'disabled') return t('routes.pool.availabilityDisabled');
  if (availability === 'cooling') return t('routes.pool.availabilityCooling');
  if (availability === 'isolated') return t('routes.pool.availabilityIsolated');
  return t('routes.pool.availabilityReady');
}

function StatusLine({ view }: { view: AdapterStatusView }) {
  return (
    <span className="flex items-center gap-1.5 text-sm">
      <StatusPin
        tone={view.tone}
        size="md"
        className={view.pulse ? 'animate-pulse' : undefined}
      />
      <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
    </span>
  );
}

export function PoolCard({
  row,
  entries,
  bridgeStatus,
  statusUnavailable,
  busy,
  error,
  active,
  targetHidden,
  onStart,
  onStop,
  onWrite,
  onShowDetail,
}: {
  row: PoolWorkbenchRow;
  entries: readonly ConnectionEntry[];
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  busy: boolean;
  error: unknown;
  active: boolean;
  targetHidden: boolean;
  onStart: (profile: AdapterProfile) => void;
  onStop: (profile: AdapterProfile) => void;
  onWrite: (profile: AdapterProfile) => void;
  onShowDetail: (profile: AdapterProfile) => void;
}) {
  const { t } = useI18n();
  const profile = row.profile;
  const entry = defaultPoolEntryUrl(row.gatewayPort);
  const surface = row.surface ? routePoolSurfaceLabel(row.surface, t) : null;
  const title = surface
    ? `${agentDisplayName(row.targetAgentId)} · ${surface}`
    : agentDisplayName(row.targetAgentId);
  const members = row.pool
    ? routePoolMemberLabels(row.pool.members, entries)
    : profile
      ? routePoolMemberLabels([{
          sourceKind: profile.sourceKind,
          sourceId: profile.sourceId,
          enabled: true,
        }], entries)
      : [];
  const listedModels = row.pool?.listedModels ?? [];
  const runtimeStatus = profile
    ? bridgeRuntimeStatusView({
        route: profile.route,
        bridgeState: bridgeStatus?.state,
        statusUnavailable,
      }, t)
    : null;
  const action = profile
    ? adapterProfilePrimaryAction({
        route: profile.route,
        bridgeState: bridgeStatus?.state,
        lastErrorCode: profile.lastErrorCode,
        statusUnavailable,
      }, t)
    : null;
  const transitioning = bridgeStatus?.state === 'starting' || bridgeStatus?.state === 'stopping';
  const failure = error ? adapterFailurePresentation(error, t('routes.mutationFailure'), t) : null;

  return (
    <ListRow
      className={LIST_ROW_PAD}
      active={active}
      onOpen={profile ? () => onShowDetail(profile) : undefined}
      data-pool-card={row.key}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="w-40 shrink-0">
          {runtimeStatus ? <StatusLine view={runtimeStatus} /> : (
            <span className="text-sm text-muted">{t('routes.pool.entry')}</span>
          )}
        </div>
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
            <AgentDot agentId={row.targetAgentId} size="sm" title={null} />
            <span className="truncate">{title}</span>
          </div>
          <p className="font-mono text-xs text-secondary">
            {entry.url ?? t('routes.pool.entryPending')}
          </p>
          <ul className="space-y-0.5">
            {members.length === 0 ? (
              <li className="text-meta text-muted">{t('routes.pool.page.noMembers')}</li>
            ) : (
              members.map((member) => (
                <li
                  key={`${member.sourceKind}:${member.sourceId}`}
                  className="flex min-w-0 flex-wrap items-center gap-x-2 text-sm"
                >
                  <span className="truncate">{member.title}</span>
                  <span className="text-meta text-muted">
                    {memberAvailabilityLabel(member.availability, member.enabled, t)}
                  </span>
                </li>
              ))
            )}
          </ul>
          {listedModels.length > 0 ? (
            <p className="text-meta text-muted">{listedModels.join(', ')}</p>
          ) : null}
          {targetHidden ? (
            <span className="text-xs text-muted">{t('routes.targetHidden')}</span>
          ) : null}
        </div>
        {profile ? (
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
                  onClick={() => (action.kind === 'stop' ? onStop(profile) : onStart(profile))}
                >
                  {busy ? t('routes.busy') : action.label}
                </Button>
              </Hint>
            ) : null}
            {profile.route === 'local_bridge' ? (
              <Button
                variant="outline"
                size="sm"
                disabled={busy || targetHidden}
                onClick={() => onWrite(profile)}
              >
                {t('routes.write.action')}
              </Button>
            ) : null}
            <Button variant="outline" size="sm" onClick={() => onShowDetail(profile)}>
              {t('routes.detail')}
            </Button>
          </div>
        ) : null}
      </div>
      {failure ? (
        <div className={cn('mt-2 space-y-1')} role="alert">
          <AdapterErrorLines error={error} fallback={t('routes.mutationFailure')} />
          <p className="text-xs text-secondary">{failure.hint}</p>
        </div>
      ) : null}
    </ListRow>
  );
}
