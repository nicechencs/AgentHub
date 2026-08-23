import { useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { SecretInput } from '@/components/shared/SecretInput';
import {
  canSubmitCreateRoute,
  CREATE_ROUTE_VENDORS,
  DEFAULT_CREATE_ROUTE_MODEL,
  defaultCreateRouteClients,
  isCreateRouteUrlValid,
  submitCreateRoute,
  urlForVendor,
  vendorIdForUrl,
  type CreateRouteClient,
  type CreateRouteTarget,
  type CreateRouteVendorId,
} from './create-route-flow';

function targetLabel(
  t: (key: string) => string,
  target: CreateRouteTarget,
): string {
  if (target === 'claude') return t('routes.create.target.claude');
  if (target === 'codex') return t('routes.create.target.codex');
  return t('routes.create.target.grok');
}

function vendorLabel(
  t: (key: string) => string,
  id: CreateRouteVendorId,
): string {
  return t(`routes.create.vendor.${id}`);
}

export function CreateRouteDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [key, setKey] = useState('');
  const [model, setModel] = useState(DEFAULT_CREATE_ROUTE_MODEL);
  const [clients, setClients] = useState<CreateRouteClient[]>(defaultCreateRouteClients);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = () => {
    setName('');
    setKey('');
    setModel(DEFAULT_CREATE_ROUTE_MODEL);
    setClients(defaultCreateRouteClients());
    setError(null);
  };

  const patchClient = (target: CreateRouteTarget, patch: Partial<CreateRouteClient>) => {
    setClients((current) =>
      current.map((row) => (row.target === target ? { ...row, ...patch } : row)),
    );
  };

  const submit = async () => {
    if (!canSubmitCreateRoute({ name, key, clients, model })) {
      const invalidEnabled = clients.some((row) => row.enabled && !isCreateRouteUrlValid(row.url));
      setError(invalidEnabled
        ? t('routes.create.urlInvalid')
        : t('routes.create.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitCreateRoute({
        name,
        key,
        clients,
        model: model.trim() || DEFAULT_CREATE_ROUTE_MODEL,
      });
      reset();
      onOpenChange(false);
      onCreated();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t('routes.create.fallback'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (busy) return;
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden">
        <DialogHeader className="shrink-0">
          <DialogTitle>{t('routes.create.title')}</DialogTitle>
          <DialogDescription>{t('routes.create.description')}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.create.name')}</span>
            <Input value={name} onChange={(event) => setName(event.target.value)} autoComplete="off" />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.create.key')}</span>
            <SecretInput value={key} onChange={setKey} />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.create.model')}</span>
            <Input
              value={model}
              onChange={(event) => setModel(event.target.value)}
              placeholder={t('routes.create.modelPlaceholder')}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <fieldset className="space-y-2">
            <legend className="text-xs text-muted">{t('routes.create.targets')}</legend>
            {clients.map((row) => (
              <div key={row.target} className="space-y-1.5 rounded-md border border-border p-2">
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={row.enabled}
                    onChange={() => patchClient(row.target, { enabled: !row.enabled })}
                  />
                  {targetLabel(t, row.target)}
                </label>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,8rem)_1fr]">
                  <label className="flex flex-col gap-1">
                    <span className="text-xs text-muted">{t('routes.create.vendorLabel')}</span>
                    <select
                      className="h-9 rounded-md border border-border bg-background px-2 text-sm"
                      value={vendorIdForUrl(row.url)}
                      disabled={!row.enabled}
                      onChange={(event) => {
                        const next = event.target.value as CreateRouteVendorId;
                        patchClient(row.target, { url: urlForVendor(next, row.url) });
                      }}
                    >
                      {CREATE_ROUTE_VENDORS.map((vendor) => (
                        <option key={vendor.id} value={vendor.id}>
                          {vendorLabel(t, vendor.id)}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex flex-col gap-1">
                    <span className="text-xs text-muted">{t('routes.create.url')}</span>
                    <Input
                      value={row.url}
                      disabled={!row.enabled}
                      onChange={(event) => patchClient(row.target, { url: event.target.value })}
                      autoComplete="off"
                      spellCheck={false}
                    />
                  </label>
                </div>
              </div>
            ))}
            <p className="text-meta text-muted">{t('routes.create.targetsHint')}</p>
          </fieldset>
          {error ? <p className="text-sm text-danger">{error}</p> : null}
        </div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button onClick={() => void submit()} disabled={busy}>
            {busy ? t('routes.create.submitting') : t('routes.create.submit')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
