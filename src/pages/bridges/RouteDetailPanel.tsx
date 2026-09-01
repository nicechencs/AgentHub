import { ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { openLogsDir } from '@/lib/api/settings';
import type {
  AdapterBridgeInboundRequest,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  DefaultRoutePoolOverview,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { fmtAbsoluteI18n } from '@/pages/backups/backup-format';
import { AdapterErrorLines } from './adapter-components';
import { InspectSurface as DialogOrSide } from '@/components/layout/InspectSurface';
import { readCreateRouteCapabilities } from './create-route-flow';
import {
  adapterBridgeHostPort,
  adapterBridgeIsListening,
  adapterCredentialKindLabel,
} from './adapter-model';
import { localTokenPreviewValue } from './client-config-model';
import {
  routeCopyPortPendingLabel,
  routeDetailTargetLabel,
  routeModelsSummary,
  routeSourceDeletedHint,
} from './adapter-route-detail-model';
import {
  buildRouteGraph,
  type RouteGraphRow,
} from './route-graph-model';
import {
  adapterProfileRecoveryGuide,
} from './adapter-view-model';
import {
  defaultPoolEntryUrl,
  localEndpointKindLabel,
  nativeEnrollCtaVisible,
  routePoolMemberLabels,
  routePoolMembersSectionVisible,
  routePoolSurfaceLabel,
} from './route-pool-view-model';
import {
  localEndpointBrandAgentId,
  localEndpointKindForTargetAgent,
  localEndpointKindFromPool,
} from '@/lib/route-endpoints';
import { InboundRequestList } from '@/components/shared/InboundRequestList';
import {
  ROUTE_LOCAL_ADDRESS_LEGEND,
} from './route-endpoint-copy';

/**
 * Route detail: login, local address, who is connected. No protocol graph.
 * `asPanel` uses the same inspect chrome as create / edit / write.
 */
export function RouteDetailPanel({
  id,
  profile,
  bridgeStatus,
  entries,
  siblingProfiles = [],
  busy,
  error,
  onRequestRemove,
  onRequestEdit,
  targetHidden = false,
  asPanel = false,
  open = true,
  onOpenChange,
  width,
  routePoolV2 = false,
  defaultPool = null,
  canApplyLocalBridge = false,
  onEnrollNative,
  enrolling = false,
}: {
  id?: string;
  profile: AdapterProfile | null;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  entries: ConnectionEntry[];
  siblingProfiles?: readonly AdapterProfile[];
  busy: boolean;
  error: unknown;
  onRequestRemove: (profile: AdapterProfile) => void;
  onRequestEdit?: (profile: AdapterProfile) => void;
  targetHidden?: boolean;
  asPanel?: boolean;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  width?: number;
  routePoolV2?: boolean;
  defaultPool?: DefaultRoutePoolOverview | null;
  canApplyLocalBridge?: boolean;
  onEnrollNative?: (profile: AdapterProfile) => void;
  enrolling?: boolean;
}) {
  const { t } = useI18n();
  if (!profile) return null;

  const requestDelete = () => {
    if (asPanel) onOpenChange?.(false);
    onRequestRemove(profile);
  };
  const deleteButton = (
    <Button
      size="sm"
      variant="dangerOutline"
      disabled={busy || targetHidden}
      title={targetHidden ? t('routes.targetHiddenHint') : undefined}
      onClick={requestDelete}
    >
      {t('routes.delete.action')}
    </Button>
  );
  const body = (
    <RouteDetailBody
      profile={profile}
      bridgeStatus={bridgeStatus}
      entries={entries}
      siblingProfiles={siblingProfiles}
      error={error}
      routePoolV2={routePoolV2}
      defaultPool={defaultPool}
      canApplyLocalBridge={canApplyLocalBridge}
      onEnrollNative={onEnrollNative}
      enrolling={enrolling}
      busy={busy}
    />
  );

  if (asPanel) {
    return (
      <DialogOrSide
        asPanel
        width={width}
        open={open}
        onOpenChange={(next) => {
          if (!next) onOpenChange?.(false);
        }}
        title={t('routes.detailTitle')}
        description={t('routes.detailDescription')}
        showCancel={false}
        primary={onRequestEdit ? (
          <Button type="button" size="sm" variant="outline" onClick={() => onRequestEdit(profile)}>
            {t('routes.edit.action')}
          </Button>
        ) : undefined}
        danger={deleteButton}
      >
        <div id={id} data-route-detail={profile.id}>
          {body}
        </div>
      </DialogOrSide>
    );
  }

  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3"
      data-route-detail={profile.id}
    >
      {body}
      <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
        {deleteButton}
      </div>
    </Card>
  );
}

function RouteDetailBody({
  profile,
  bridgeStatus,
  entries,
  siblingProfiles,
  error,
  routePoolV2 = false,
  defaultPool = null,
  canApplyLocalBridge = false,
  onEnrollNative,
  enrolling = false,
  busy = false,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  entries: ConnectionEntry[];
  siblingProfiles: readonly AdapterProfile[];
  error: unknown;
  routePoolV2?: boolean;
  defaultPool?: DefaultRoutePoolOverview | null;
  canApplyLocalBridge?: boolean;
  onEnrollNative?: (profile: AdapterProfile) => void;
  enrolling?: boolean;
  busy?: boolean;
}) {
  const { toast } = useToast();
  const { t, lang } = useI18n();
  const isBridge = profile.route === 'local_bridge';
  const endpointParts = isBridge ? adapterBridgeHostPort(profile, bridgeStatus) : null;
  const graph = buildRouteGraph({
    profile,
    entries,
    siblingProfiles,
    host: endpointParts?.host,
    port: endpointParts?.port,
  });
  const source = graph.source;
  const recovery = adapterProfileRecoveryGuide(profile, t);
  const sourceEntry = entries.find(
    (entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId,
  );
  const capabilities = readCreateRouteCapabilities(sourceEntry?.provider?.configText);

  return (
    <>
      <div className="space-y-4">
        <section className="space-y-2">
          <h3 className="text-body font-medium">{t('routes.graph.mappingTitle')}</h3>
          <div className={cn(
            'rounded-card border border-border bg-subtle p-3 space-y-3',
            source.missing && 'opacity-70',
          )}
          >
            <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
              {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
              <span className="truncate">{source.title}</span>
              <Badge variant="default">{adapterCredentialKindLabel(profile.mode, t)}</Badge>
            </div>
            {source.missing ? (
              <p className="text-sm text-warning">{routeSourceDeletedHint(t)}</p>
            ) : null}

            <dl className="grid gap-2 text-sm">
              {source.baseUrl ? (
              <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
                <dt className="w-12 shrink-0 text-muted">{t('routes.graph.upstreamTitle')}</dt>
                <dd className="min-w-0">
                    <CopyableEndpoint
                      text={source.baseUrl}
                      url={source.baseUrl}
                      ariaLabel={t('routes.graph.copyUpstream', { endpoint: source.baseUrl })}
                    />
                </dd>
              </div>
              ) : null}
              <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
                <dt className="w-12 shrink-0 text-muted">{t('routes.graph.localTitle')}</dt>
                <dd className="min-w-0">
                  {graph.local.origin ? (
                    <CopyableEndpoint
                      text={graph.local.origin}
                      url={graph.local.origin}
                      ariaLabel={t('routes.graph.copyLocal', { endpoint: graph.local.origin })}
                      className="text-sm font-medium"
                    />
                  ) : (
                    <span className="text-muted">
                      {isBridge && !adapterBridgeIsListening(bridgeStatus)
                        ? t('routes.bridgeState.stopped')
                        : routeCopyPortPendingLabel(t)}
                    </span>
                  )}
                </dd>
              </div>
              {isBridge ? (
                <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
                  <dt className="w-12 shrink-0 text-muted">{t('routes.write.fieldLocalToken')}</dt>
                  <dd className="min-w-0">
                    {bridgeStatus?.localToken ? (
                      <CopyableEndpoint
                        text={localTokenPreviewValue(bridgeStatus.localToken, t)}
                        url={bridgeStatus.localToken}
                        ariaLabel={t('routes.write.fieldLocalToken')}
                        className="text-sm font-medium"
                        sensitive
                      />
                    ) : (
                      <span className="text-muted">{t('routes.write.localToken')}</span>
                    )}
                  </dd>
                </div>
              ) : null}
            </dl>

            <div className="space-y-1">
              <h4 className="text-sm font-medium">{t('routes.endpoint.legendTitle')}</h4>
              <ul className="space-y-0.5 text-meta text-muted">
                {ROUTE_LOCAL_ADDRESS_LEGEND.map((row) => (
                  <li key={row.path} className="font-mono">
                    {row.method} {row.path} · {t(row.copyKey)}
                  </li>
                ))}
              </ul>
              <p className="text-meta text-muted">{t('routes.endpoint.hint')}</p>
            </div>

            <div className="space-y-1.5">
              <h4 className="text-sm font-medium">{t('routes.graph.clientsTitle')}</h4>
              {graph.rows.length === 0 ? (
                <p className="text-sm text-muted">{t('routes.graph.empty')}</p>
              ) : (
                <ul className="space-y-1">
                  {graph.rows.map((row) => (
                    <ClientRow key={row.agent} row={row} />
                  ))}
                </ul>
              )}
            </div>
          </div>
          <p className="text-meta text-muted">{routeModelsSummary(capabilities.models, t)}</p>
        </section>

        <InboundRequestsSection rows={bridgeStatus?.recentInbound ?? []} />

        {routePoolMembersSectionVisible(routePoolV2, defaultPool) && defaultPool ? (
          <RoutePoolOverviewSection
            pool={defaultPool}
            entries={entries}
            localToken={bridgeStatus?.localToken}
          />
        ) : null}

        {nativeEnrollCtaVisible({
          flagOn: routePoolV2,
          route: profile.route,
          canApplyLocalBridge,
        }) && onEnrollNative ? (
          <section className="space-y-2">
            <Button
              type="button"
              size="sm"
              disabled={busy || enrolling}
              onClick={() => onEnrollNative(profile)}
            >
              {enrolling ? t('routes.pool.enrolling') : t('routes.pool.enrollNative')}
            </Button>
          </section>
        ) : null}

        {recovery ? (
          <section className="space-y-1.5" role="status">
            <h3 className="text-sm font-medium text-warning">{t('routes.recovery.stepsTitle')}</h3>
            <p className="text-sm text-secondary">{recovery.summary}</p>
            <ul className="list-disc space-y-0.5 pl-5 text-sm text-secondary">
              {recovery.steps.map((step) => <li key={step}>{step}</li>)}
            </ul>
          </section>
        ) : null}

        {error ? <AdapterErrorLines error={error} fallback={t('routes.mutationFailure')} /> : null}

        <details className="group rounded-card border border-border bg-subtle/60">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
            <span className="inline-flex items-center gap-1.5">
              {t('routes.diagnostics')}
              {profile.lastErrorCode ? (
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-danger" aria-hidden />
              ) : null}
            </span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" aria-hidden />
          </summary>
          <div className="grid gap-1.5 border-t border-border px-3 py-3 text-xs">
            <DetailRow label={t('routes.profileId')} value={profile.id} mono />
            <DetailRow label={t('routes.rule')} value={`${profile.ruleId} · v${profile.ruleVersion}`} mono />
            {profile.lastErrorCode ? <DetailRow label={t('routes.lastError')} value={profile.lastErrorCode} mono /> : null}
            <DetailRow label={t('routes.createdAt')} value={fmtAbsoluteI18n(profile.createdAt, lang)} mono />
            <DetailRow label={t('routes.updatedAt')} value={fmtAbsoluteI18n(profile.updatedAt, lang)} mono />
            {source.upstreamUrls.map((url) => (
              <DetailRow key={url} label={t('routes.panel.upstreamUrl')} value={url} mono />
            ))}
            <div>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  void (async () => {
                    try {
                      const path = await openLogsDir();
                      toast({ title: t('routes.logsOpened'), description: path, variant: 'success' });
                    } catch (openError) {
                      toast({ title: t('routes.openFailed'), description: String(openError), variant: 'danger' });
                    }
                  })();
                }}
              >
                {t('routes.openLogs')}
              </Button>
            </div>
          </div>
        </details>
      </div>
    </>
  );
}

function RoutePoolOverviewSection({
  pool,
  entries,
  localToken,
}: {
  pool: DefaultRoutePoolOverview;
  entries: ConnectionEntry[];
  localToken?: string | null;
}) {
  const { t } = useI18n();
  const entry = defaultPoolEntryUrl(pool.gatewayPort);
  const endpointKind = localEndpointKindFromPool(pool);
  const members = routePoolMemberLabels(
    pool.members,
    entries,
    t('routes.pool.detail.identityUnavailable'),
  );
  return (
    <section className="space-y-2" data-route-pool={pool.id}>
      <h3 className="text-body font-medium">{t('routes.pool.entry')}</h3>
      <div className="rounded-card border border-border bg-subtle p-3 space-y-3">
        <dl className="grid gap-2 text-sm">
          <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
            <dt className="w-12 shrink-0 text-muted">{t('routes.pool.entry')}</dt>
            <dd className="min-w-0">
              {entry.url ? (
                <CopyableEndpoint
                  text={entry.url}
                  url={entry.url}
                  ariaLabel={t('routes.graph.copyLocal', { endpoint: entry.url })}
                  className="text-sm font-medium"
                />
              ) : (
                <span className="text-muted">{t('routes.pool.entryPending')}</span>
              )}
            </dd>
          </div>
          <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
            <dt className="w-12 shrink-0 text-muted">{t('routes.pool.surfaceLabel')}</dt>
            <dd className="min-w-0 text-sm">{
              endpointKind
                ? localEndpointKindLabel(endpointKind, t)
                : routePoolSurfaceLabel(pool.surface, t)
            }</dd>
          </div>
          {pool.listedModels && pool.listedModels.length > 0 ? (
            <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
              <dt className="w-12 shrink-0 text-muted">{t('routes.capabilities.models')}</dt>
              <dd className="min-w-0 text-sm">{pool.listedModels.join(', ')}</dd>
            </div>
          ) : null}
        </dl>
        <div className="space-y-1.5">
          <h4 className="text-sm font-medium">{t('routes.pool.members')}</h4>
          {members.length === 0 ? (
            <p className="text-sm text-muted">{t('routes.graph.empty')}</p>
          ) : (
            <ul className="space-y-1">
              {members.map((member) => (
                <li
                  key={`${member.sourceKind}:${member.sourceId}`}
                  className="flex min-w-0 flex-wrap items-center gap-x-2 py-0.5 text-sm"
                >
                  <span className="truncate">{member.title}</span>
                  {member.availability && member.availability !== 'ready' ? (
                    <span className="text-meta text-muted">
                      {member.availability === 'cooling'
                        ? t('routes.pool.availabilityCooling')
                        : member.availability === 'isolated'
                          ? t('routes.pool.availabilityIsolated')
                          : t('routes.pool.availabilityDisabled')}
                    </span>
                  ) : member.enabled ? null : (
                    <span className="text-meta text-muted">{t('routes.pool.memberOff')}</span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {localToken ? (
          <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
            <dt className="w-12 shrink-0 text-muted">{t('routes.write.fieldLocalToken')}</dt>
            <dd className="min-w-0">
              <CopyableEndpoint
                text={localTokenPreviewValue(localToken, t)}
                url={localToken}
                ariaLabel={t('routes.write.fieldLocalToken')}
                className="text-sm font-medium"
                sensitive
              />
            </dd>
          </div>
        ) : (
          <p className="text-meta text-muted">{t('routes.pool.tokenSaved')}</p>
        )}
      </div>
    </section>
  );
}

function ClientRow({ row }: { row: RouteGraphRow }) {
  const { t } = useI18n();
  const label = routeDetailTargetLabel(row.agent, t);
  const url = row.localUrl ?? '';
  const endpointKind = localEndpointKindForTargetAgent(row.agent);
  const brandAgentId = localEndpointBrandAgentId(endpointKind);
  return (
    <li
      className={cn(
        'flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 py-1',
        !row.enabled && 'opacity-70',
      )}
    >
      <span className="flex min-w-[5.5rem] items-center gap-1.5 text-sm font-medium">
        <AgentDot agentId={row.agent} size="sm" title={null} />
        <span className="truncate">{label}</span>
      </span>
      {row.applied ? (
        <span className="shrink-0 text-meta text-success">{t('routes.graph.applied')}</span>
      ) : !row.enabled ? (
        <span className="shrink-0 text-meta text-muted">{t('routes.graph.notEnabled')}</span>
      ) : (
        <span className="shrink-0 text-meta text-muted">{t('routes.graph.notWritten')}</span>
      )}
      <span className="min-w-0 flex-1">
        <CopyableEndpoint
          text={url || row.localPath}
          url={url}
          ariaLabel={t('routes.graph.copyLocal', { endpoint: url || row.localPath })}
        />
        <RouteEndpointTypeText
          endpointId={row.localEndpointId}
          brandAgentId={brandAgentId}
          className="ml-1 text-meta"
        >
          {localEndpointKindLabel(endpointKind, t)}
        </RouteEndpointTypeText>
      </span>
    </li>
  );
}

function InboundRequestsSection({ rows }: { rows: readonly AdapterBridgeInboundRequest[] }) {
  const { t } = useI18n();
  return (
    <section className="space-y-2" data-route-inbound>
      <h3 className="text-body font-medium">{t('routes.inbound.title')}</h3>
      <InboundRequestList rows={rows} emptyLabel={t('routes.inbound.empty')} />
    </section>
  );
}

function CopyableEndpoint({
  text,
  url,
  ariaLabel,
  className,
  sensitive = false,
}: {
  text: string;
  url: string;
  ariaLabel: string;
  className?: string;
  sensitive?: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const canCopy = Boolean(url.trim());
  const hint = canCopy
    ? (sensitive ? text : url)
    : routeCopyPortPendingLabel(t);
  return (
    <Hint label={hint}>
      <button
        type="button"
        className="inline-flex max-w-full items-center gap-1 rounded-btn px-1 py-0.5 text-left hover:bg-hover disabled:cursor-not-allowed disabled:opacity-60"
        disabled={!canCopy}
        aria-label={ariaLabel}
        onClick={() => {
          if (!canCopy) return;
          void (async () => {
            try {
              await navigator.clipboard.writeText(url);
              toast({
                title: t('routes.endpointCopied'),
                description: sensitive ? text : url,
              });
            } catch {
              toast({ title: t('routes.copyFailed'), variant: 'danger' });
            }
          })();
        }}
      >
        <span className={cn('truncate font-mono text-xs text-secondary', className)}>{text}</span>
        <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
      </button>
    </Hint>
  );
}
