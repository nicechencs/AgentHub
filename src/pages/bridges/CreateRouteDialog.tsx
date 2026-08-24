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
import type { TranslateFn } from '@/lib/i18n';
import {
  canSubmitCreateRoute,
  CREATE_ROUTE_TARGETS,
  CREATE_ROUTE_VENDORS,
  DEFAULT_CREATE_ROUTE_MODEL,
  createRouteAutoNames,
  defaultCreateRouteEndpoints,
  defaultCreateRouteName,
  endpointUrlFor,
  nextCreateRouteName,
  formatCreateRouteModels,
  isCreateRouteUrlValid,
  submitCreateRoute,
  vendorById,
  type CreateRouteTarget,
  type CreateRouteVendorId,
} from './create-route-flow';

function targetLabel(t: TranslateFn, target: CreateRouteTarget): string {
  if (target === 'claude') return t('routes.create.target.claude');
  if (target === 'codex') return t('routes.create.target.codex');
  return t('routes.create.target.grok');
}

function vendorLabel(t: TranslateFn, id: CreateRouteVendorId): string {
  switch (id) {
    case 'openrouter':
      return t('routes.create.vendor.openrouter');
    case 'openai':
      return t('routes.create.vendor.openai');
    case 'glm':
      return t('routes.create.vendor.glm');
    case 'kimi':
      return t('routes.create.vendor.kimi');
    case 'deepseek':
      return t('routes.create.vendor.deepseek');
    case 'grok':
      return t('routes.create.vendor.grok');
    case 'claude':
      return t('routes.create.vendor.claude');
    case 'custom':
      return t('routes.create.vendor.custom');
  }
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
  const [vendor, setVendor] = useState<CreateRouteVendorId>('openrouter');
  const [name, setName] = useState('');
  const [url, setUrl] = useState(vendorById('openrouter').url);
  const [key, setKey] = useState('');
  const [models, setModels] = useState(formatCreateRouteModels(vendorById('openrouter').models));
  const [endpoints, setEndpoints] = useState<CreateRouteTarget[]>(defaultCreateRouteEndpoints('openrouter'));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createInput = { name, url, key, vendor, endpoints, models };
  const canSubmit = canSubmitCreateRoute(createInput);

  const reset = () => {
    setVendor('openrouter');
    setName('');
    setUrl(vendorById('openrouter').url);
    setKey('');
    setModels(formatCreateRouteModels(vendorById('openrouter').models));
    setEndpoints(defaultCreateRouteEndpoints('openrouter'));
    setError(null);
  };

  const applyVendor = (next: CreateRouteVendorId) => {
    const spec = vendorById(next);
    const alternate = t('routes.create.alternate');
    const autoNames = createRouteAutoNames(
      CREATE_ROUTE_VENDORS.map((item) => vendorLabel(t, item.id)),
      alternate,
    );
    setVendor(next);
    setName(nextCreateRouteName(
      name,
      defaultCreateRouteName(vendorLabel(t, next), alternate),
      autoNames,
    ));
    if (next === 'custom') return;
    setUrl(spec.url);
    setEndpoints([...spec.enabled]);
    setModels(formatCreateRouteModels(spec.models));
  };

  const toggleEndpoint = (target: CreateRouteTarget) => {
    setEndpoints((current) =>
      current.includes(target)
        ? current.filter((item) => item !== target)
        : [...current, target],
    );
  };

  const submitCreate = async () => {
    if (busy) return;
    if (!canSubmitCreateRoute(createInput)) {
      setError(url.trim() && !isCreateRouteUrlValid(url)
        ? t('routes.create.urlInvalid')
        : t('routes.create.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitCreateRoute({
        ...createInput,
        models: vendor === 'openrouter' ? (models.trim() || DEFAULT_CREATE_ROUTE_MODEL) : models,
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
      <DialogContent
        className="flex max-h-[min(36rem,calc(100vh-2rem))] flex-col overflow-hidden"
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
        onFocusOutside={(event) => event.preventDefault()}
      >
        <form
          className="flex min-h-0 flex-1 flex-col"
          onSubmit={(event) => {
            event.preventDefault();
            if (busy || !canSubmit) return;
            void submitCreate();
          }}
        >
          <DialogHeader className="shrink-0">
            <DialogTitle>{t('routes.create.title')}</DialogTitle>
            <DialogDescription>{t('routes.create.description')}</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain pr-1 pb-1">
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.vendorLabel')}</span>
              <select
                className="h-9 rounded-btn border border-border bg-background px-2 text-sm"
                value={vendor}
                onChange={(event) => applyVendor(event.target.value as CreateRouteVendorId)}
              >
                {CREATE_ROUTE_VENDORS.map((item) => (
                  <option key={item.id} value={item.id}>
                    {vendorLabel(t, item.id)}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.name')}</span>
              <Input value={name} onChange={(event) => setName(event.target.value)} autoComplete="off" />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.url')}</span>
              <Input
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </label>
            <div className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.key')}</span>
              <SecretInput value={key} onChange={setKey} />
            </div>
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.models')}</span>
              <Input
                value={models}
                onChange={(event) => setModels(event.target.value)}
                placeholder={t('routes.create.modelsPlaceholder')}
                autoComplete="off"
                spellCheck={false}
              />
              <p className="text-meta text-muted">{t('routes.create.modelsHint')}</p>
            </label>
            <fieldset className="space-y-2">
              <legend className="text-xs text-muted">{t('routes.create.targets')}</legend>
              {CREATE_ROUTE_TARGETS.map((target) => (
                <label key={target} className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    className="mt-0.5"
                    checked={endpoints.includes(target)}
                    onChange={() => toggleEndpoint(target)}
                  />
                  <span className="min-w-0">
                    <span className="block">{targetLabel(t, target)}</span>
                    <span className="block break-all text-meta text-muted">
                      {endpointUrlFor(vendor, target, url)}
                    </span>
                  </span>
                </label>
              ))}
              <p className="text-meta text-muted">{t('routes.create.targetsHint')}</p>
            </fieldset>
            {error ? <p className="text-sm text-danger">{error}</p> : null}
          </div>
          <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={busy || !canSubmit}>
              {busy ? t('routes.create.submitting') : t('routes.create.submit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
