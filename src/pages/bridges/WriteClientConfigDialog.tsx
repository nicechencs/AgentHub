import { useEffect, useRef, useState } from 'react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
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
import { useToast } from '@/components/ui/toast';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { cn } from '@/lib/utils';
import { AdapterErrorLines } from './adapter-components';
import {
  buildClientWriteSpecs,
  canWriteClientConfig,
  clientWriteAgentLabel,
  clientWriteStatusLabel,
  clientWriteWireNote,
  defaultClientWriteSelection,
  orderedClientWriteTargets,
  type ClientWriteSpec,
  type ClientWriteStatus,
} from './client-config-model';
import { applyLocalRouteToAgents, type CreateRouteTarget } from './create-route-flow';
import type { RouteGraphRow } from './route-graph-model';

function statusVariant(status: ClientWriteStatus): 'success' | 'accent' | 'default' {
  if (status === 'applied') return 'success';
  if (status === 'ready') return 'accent';
  return 'default';
}

export function WriteClientConfigDialog({
  open,
  onOpenChange,
  profile,
  rows,
  host,
  port,
  sourceMissing,
  hiddenTargetIds,
  onWritten,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  profile: AdapterProfile | null;
  rows: readonly RouteGraphRow[];
  host?: string;
  port?: number | null;
  sourceMissing: boolean;
  hiddenTargetIds?: ReadonlySet<string>;
  /** Reload the page's route list after a successful write. */
  onWritten: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [selected, setSelected] = useState<CreateRouteTarget[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [missingSelection, setMissingSelection] = useState(false);

  const specs = buildClientWriteSpecs({ rows, host, port, sourceMissing, hiddenTargetIds, t });
  const specsRef = useRef<ClientWriteSpec[]>(specs);
  specsRef.current = specs;
  const profileId = profile?.id ?? null;
  const portPending = !(typeof port === 'number' && port > 0);

  useEffect(() => {
    if (!open) return;
    setSelected(defaultClientWriteSelection(specsRef.current));
    setError(null);
    setMissingSelection(false);
  }, [open, profileId]);

  if (!profile) return null;

  const toggle = (agent: CreateRouteTarget) => {
    setMissingSelection(false);
    setSelected((current) => (
      current.includes(agent)
        ? current.filter((item) => item !== agent)
        : [...current, agent]
    ));
  };

  const submit = async () => {
    if (busy) return;
    if (!canWriteClientConfig(selected)) {
      setError(null);
      setMissingSelection(true);
      return;
    }
    setBusy(true);
    setError(null);
    setMissingSelection(false);
    try {
      await applyLocalRouteToAgents({
        sourceKind: profile.sourceKind,
        sourceId: profile.sourceId,
        agents: orderedClientWriteTargets(selected),
      });
      toast({ title: t('routes.write.success'), variant: 'success' });
      onOpenChange(false);
      onWritten();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => closeConfirmationOnOpenChange(next, busy, () => onOpenChange(false))}
    >
      <DialogContent
        className="flex max-h-[min(36rem,calc(100vh-2rem))] flex-col overflow-hidden"
        hideClose={busy}
        onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
        onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        onInteractOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>{t('routes.write.title')}</DialogTitle>
          <DialogDescription>{t('routes.write.description')}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain pr-1 pb-1">
          {portPending ? <p className="text-meta text-muted">{t('routes.write.portPending')}</p> : null}
          <fieldset className="space-y-2">
            <legend className="text-xs text-muted">{t('routes.write.selectLabel')}</legend>
            <ul className="space-y-2">
              {specs.map((spec) => (
                <WriteTargetRow
                  key={spec.agent}
                  spec={spec}
                  host={host}
                  port={port}
                  checked={selected.includes(spec.agent)}
                  disabled={busy || !spec.selectable}
                  onToggle={() => toggle(spec.agent)}
                />
              ))}
            </ul>
          </fieldset>
          {missingSelection ? (
            <p className="text-sm text-danger" role="alert">{t('routes.write.required')}</p>
          ) : null}
          {error ? <AdapterErrorLines error={error} fallback={t('routes.write.fallback')} /> : null}
        </div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button onClick={() => void submit()} disabled={busy || !canWriteClientConfig(selected)}>
            {busy
              ? t('routes.write.submitting')
              : t('routes.write.submit', { count: selected.length })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function WriteTargetRow({
  spec,
  host,
  port,
  checked,
  disabled,
  onToggle,
}: {
  spec: ClientWriteSpec;
  host?: string;
  port?: number | null;
  checked: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const label = clientWriteAgentLabel(spec.agent, t);

  return (
    <li
      className={cn(
        'space-y-1.5 rounded-card border border-border bg-subtle p-3',
        !spec.selectable && 'opacity-70',
      )}
    >
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <label className="inline-flex items-center gap-1.5">
          <input
            type="checkbox"
            checked={checked}
            disabled={disabled}
            onChange={onToggle}
          />
          <AgentDot agentId={spec.agent} size="sm" title={null} />
          <span className="text-sm font-medium">{label}</span>
        </label>
        <Badge variant={statusVariant(spec.status)}>
          {clientWriteStatusLabel(spec.status, label, t)}
        </Badge>
      </div>
      <dl className="grid grid-cols-[5.5rem_minmax(0,1fr)] gap-x-2 gap-y-1 text-meta">
        <dt className="text-muted">{t('routes.write.endpointLabel')}</dt>
        <dd className="min-w-0">
          <CopyableRouteEndpointUrl
            path={spec.endpointPath}
            port={port}
            host={host}
            endpointId={spec.endpointId}
            className="text-meta"
          />
        </dd>
        <dt className="text-muted">{t('routes.write.wireLabel')}</dt>
        <dd className="min-w-0 text-secondary">{clientWriteWireNote(spec.agent, t)}</dd>
        <dt className="text-muted">{t('routes.write.configPathLabel')}</dt>
        <dd className="min-w-0 break-all font-mono text-secondary">{spec.configPath}</dd>
        <dt className="text-muted">{t('routes.write.fieldsLabel')}</dt>
        <dd className="min-w-0 space-y-0.5 font-mono text-secondary">
          {spec.fields.map((field) => (
            <span key={field.key} className="block break-all">
              {field.key} = {field.value}
            </span>
          ))}
        </dd>
      </dl>
    </li>
  );
}
