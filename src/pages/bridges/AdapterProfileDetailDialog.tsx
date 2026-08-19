import { ArrowRight, ChevronDown, Copy } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { DetailRow } from '@/components/shared/DetailRow';
import { useI18n } from '@/components/shared/LanguageProvider';
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
import { Switch } from '@/components/ui/switch';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import { openLogsDir } from '@/lib/api/settings';
import type {
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { AdapterErrorLines } from './adapter-components';
import {
  adapterBridgeEndpointLabel,
  adapterBridgeUpstreamLabel,
  adapterCredentialKindLabel,
} from './adapter-model';
import {
  adapterProfileRecoveryGuide,
  adapterStatusDotClass,
  adapterStatusTextClass,
  bridgeRuntimeStatusView,
  resolveAdapterProfileSource,
  type AdapterStatusView,
} from './adapter-view-model';

/**
 * Read-only runtime detail. AutoStart is the only editable field the backend
 * exposes, so it lives here as a direct switch (no edit mode / dirty state).
 * Unbind is requested from here and confirmed by the page-level dialog.
 */
export function AdapterProfileDetailDialog({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  targetHidden = false,
}: {
  profile: AdapterProfile | null;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  targetHidden?: boolean;
}) {
  return (
    <Dialog open={Boolean(profile)} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden">
        {profile ? (
          <ProfileDetailBody
            profile={profile}
            bridgeStatus={bridgeStatus}
            statusUnavailable={statusUnavailable}
            entries={entries}
            busy={busy}
            error={error}
            onClose={onClose}
            onSetAutoStart={onSetAutoStart}
            onRequestRemove={onRequestRemove}
            targetHidden={targetHidden}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function ProfileDetailBody({
  profile,
  bridgeStatus,
  statusUnavailable,
  entries,
  busy,
  error,
  onClose,
  onSetAutoStart,
  onRequestRemove,
  targetHidden,
}: {
  profile: AdapterProfile;
  bridgeStatus?: AdapterBridgeRuntimeStatus;
  statusUnavailable: boolean;
  entries: ConnectionEntry[];
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSetAutoStart: (profile: AdapterProfile, autoStart: boolean) => void;
  onRequestRemove: (profile: AdapterProfile) => void;
  targetHidden: boolean;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  const source = resolveAdapterProfileSource(profile, entries);
  const runtimeStatus = bridgeRuntimeStatusView({
    route: profile.route,
    bridgeState: bridgeStatus?.state,
    statusUnavailable,
  }, t);
  const isBridge = profile.route === 'local_bridge';
  const endpoint = isBridge ? adapterBridgeEndpointLabel(profile, bridgeStatus) : null;
  const recovery = adapterProfileRecoveryGuide(profile, t);

  const copyEndpoint = async () => {
    if (!endpoint) return;
    try {
      await navigator.clipboard.writeText(`http://${endpoint}`);
      toast({ title: t('routes.endpointCopied'), description: `http://${endpoint}` });
    } catch {
      toast({ title: t('routes.copyFailed'), variant: 'danger' });
    }
  };

  return (
    <>
      <DialogHeader className="shrink-0">
        <DialogTitle className="flex min-w-0 flex-wrap items-center gap-1.5">
          {source.agentId ? <AgentDot agentId={source.agentId} size="sm" title={null} /> : null}
          <span className="truncate">{source.title}</span>
          <ArrowRight className="h-4 w-4 shrink-0 text-muted" aria-hidden />
          <AgentDot agentId={profile.targetAgentId} size="sm" title={null} />
          <span className="truncate">{agentDisplayName(profile.targetAgentId)}</span>
        </DialogTitle>
        <DialogDescription className="flex flex-wrap items-center gap-1.5">
          <Badge variant="default">{adapterCredentialKindLabel(profile.mode, t)}</Badge>
          {source.missing ? <span className="text-warning">{t('routes.sourceDeleted')}</span> : null}
        </DialogDescription>
      </DialogHeader>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
        <section className="space-y-1.5">
          <h3 className="text-sm font-medium">{t('routes.status')}</h3>
          <div className="space-y-1 rounded-btn border border-border bg-subtle p-3">
            {runtimeStatus ? <DetailStatusLine view={runtimeStatus} /> : null}
          </div>
        </section>

        {isBridge ? (
          <section className="space-y-1.5">
            <h3 className="text-sm font-medium">{t('routes.localEndpoint')}</h3>
            <div className="space-y-2 rounded-btn border border-border bg-subtle p-3 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-muted">{t('routes.localEndpointLabel')}</span>
                {endpoint ? (
                  <button
                    type="button"
                    className="inline-flex items-center gap-1 rounded-btn px-1 py-0.5 font-mono text-xs text-secondary hover:bg-hover hover:text-primary"
                    onClick={() => { void copyEndpoint(); }}
                    aria-label={t('routes.copyEndpointAria', { endpoint })}
                  >
                    {endpoint}
                    <Copy className="h-3 w-3" aria-hidden />
                  </button>
                ) : (
                  <span className="text-xs text-muted">{t('routes.pendingPort')}</span>
                )}
              </div>
              {bridgeStatus?.upstreamStatus ? (
                <DetailRow
                  label={t('routes.upstreamStatus')}
                  value={adapterBridgeUpstreamLabel(bridgeStatus.upstreamStatus, t)}
                />
              ) : null}
              <label className="flex items-center justify-between gap-2 text-sm">
                <span className="min-w-0">
                  <span className="block">{t('routes.autoStart')}</span>
                  <span className="block text-xs text-muted">{t('routes.autoStartHint')}</span>
                </span>
                <Switch
                  checked={profile.autoStart}
                  disabled={busy || targetHidden}
                  aria-label={t('routes.autoStart')}
                  title={targetHidden ? t('routes.targetHiddenHint') : undefined}
                  onCheckedChange={(autoStart) => onSetAutoStart(profile, autoStart)}
                />
              </label>
            </div>
          </section>
        ) : null}

        <section className="space-y-1.5">
          <h3 className="text-sm font-medium">{t('routes.targetWrite')}</h3>
          <p className="text-sm text-secondary">
            {profile.generatedProviderId
              ? t('routes.writtenTo', { name: agentDisplayName(profile.targetAgentId) })
              : t('routes.notWritten')}
          </p>
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

        <details className="group rounded-btn border border-border bg-subtle/60">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
            <span>{t('routes.diagnostics')}</span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" aria-hidden />
          </summary>
          <div className="grid gap-1.5 border-t border-border px-3 py-3 text-xs">
            <DetailRow label={t('routes.profileId')} value={profile.id} mono />
            <DetailRow label={t('routes.rule')} value={`${profile.ruleId} · v${profile.ruleVersion}`} mono />
            {profile.lastErrorCode ? <DetailRow label={t('routes.lastError')} value={profile.lastErrorCode} mono /> : null}
            <DetailRow label={t('routes.createdAt')} value={profile.createdAt} mono />
            <DetailRow label={t('routes.updatedAt')} value={profile.updatedAt} mono />
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

      <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
        <Button
          variant="dangerOutline"
          disabled={busy || targetHidden}
          title={targetHidden ? t('routes.targetHiddenHint') : undefined}
          onClick={() => onRequestRemove(profile)}
        >
          {t('routes.unbind.action')}
        </Button>
        <Button variant="secondary" onClick={onClose}>{t('routes.close')}</Button>
      </DialogFooter>
    </>
  );
}

function DetailStatusLine({ view }: { view: AdapterStatusView }) {
  return (
    <p className="flex items-center gap-2 text-sm">
      <span
        className={`inline-block h-2 w-2 shrink-0 rounded-full ${adapterStatusDotClass(view.tone)}${view.pulse ? ' animate-pulse' : ''}`}
        aria-hidden
      />
      <span className={adapterStatusTextClass(view.tone)}>{view.label}</span>
    </p>
  );
}
