import { ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { openLogsDir } from '@/lib/api/settings';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from './adapter-components';
import { readCreateRouteCapabilities } from './create-route-flow';
import {
  adapterBridgeHostPort,
  adapterCredentialKindLabel,
} from './adapter-model';
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

/**
 * Route detail: login, local address, who is connected. No protocol graph.
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
  targetHidden = false,
}: {
  id?: string;
  profile: AdapterProfile | null;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  entries: ConnectionEntry[];
  siblingProfiles?: readonly AdapterProfile[];
  busy: boolean;
  error: unknown;
  onRequestRemove: (profile: AdapterProfile) => void;
  targetHidden?: boolean;
}) {
  if (!profile) return null;
  return (
    <Card
      id={id}
      variant="plain"
      className="mt-3 flex flex-col gap-3 bg-canvas p-3"
      data-route-detail={profile.id}
    >
      <RouteDetailBody
        profile={profile}
        bridgeStatus={bridgeStatus}
        entries={entries}
        siblingProfiles={siblingProfiles}
        busy={busy}
        error={error}
        onRequestRemove={onRequestRemove}
        targetHidden={targetHidden}
      />
    </Card>
  );
}

function RouteDetailBody({
  profile,
  bridgeStatus,
  entries,
  siblingProfiles,
  busy,
  error,
  onRequestRemove,
  targetHidden,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  entries: ConnectionEntry[];
  siblingProfiles: readonly AdapterProfile[];
  busy: boolean;
  error: unknown;
  onRequestRemove: (profile: AdapterProfile) => void;
  targetHidden: boolean;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
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
                    <span className="text-muted">{routeCopyPortPendingLabel(t)}</span>
                  )}
                </dd>
              </div>
            </dl>

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
            <DetailRow label={t('routes.createdAt')} value={profile.createdAt} mono />
            <DetailRow label={t('routes.updatedAt')} value={profile.updatedAt} mono />
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

      <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
        <Button
          size="sm"
          variant="dangerOutline"
          disabled={busy || targetHidden}
          title={targetHidden ? t('routes.targetHiddenHint') : undefined}
          onClick={() => onRequestRemove(profile)}
        >
          {t('routes.delete.action')}
        </Button>
      </div>
    </>
  );
}

function ClientRow({ row }: { row: RouteGraphRow }) {
  const { t } = useI18n();
  const label = routeDetailTargetLabel(row.agent, t);
  const url = row.localUrl ?? '';
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
      </span>
    </li>
  );
}

function CopyableEndpoint({
  text,
  url,
  ariaLabel,
  className,
}: {
  text: string;
  url: string;
  ariaLabel: string;
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const canCopy = Boolean(url.trim());
  return (
    <Hint label={canCopy ? url : routeCopyPortPendingLabel(t)}>
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
              toast({ title: t('routes.endpointCopied'), description: url });
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
