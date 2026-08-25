import { useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { InspectSurface as DialogOrSide } from '@/components/layout/InspectSurface';
import { Input } from '@/components/ui/input';
import { SecretInput } from '@/components/shared/SecretInput';
import type { TranslateFn } from '@/lib/i18n';
import type { ClaudeContextWindowChoice } from '@/lib/claude-client-env';
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
  asPanel = false,
  width,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
  asPanel?: boolean;
  width?: number;
}) {
  const { t } = useI18n();
  const [vendor, setVendor] = useState<CreateRouteVendorId>('openrouter');
  const [name, setName] = useState('');
  const [url, setUrl] = useState(vendorById('openrouter').url);
  const [key, setKey] = useState('');
  const [models, setModels] = useState(formatCreateRouteModels(vendorById('openrouter').models));
  const [contextWindow, setContextWindow] = useState<ClaudeContextWindowChoice>(
    vendorById('openrouter').defaultContextWindow ?? 'auto',
  );
  const [endpoints, setEndpoints] = useState<CreateRouteTarget[]>(defaultCreateRouteEndpoints('openrouter'));
  const [endpointUrls, setEndpointUrls] = useState<Partial<Record<CreateRouteTarget, string>>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createInput = { name, url, key, vendor, endpoints, models, contextWindow, endpointUrls };
  const canSubmit = canSubmitCreateRoute(createInput);

  const reset = () => {
    setVendor('openrouter');
    setName('');
    setUrl(vendorById('openrouter').url);
    setKey('');
    setModels(formatCreateRouteModels(vendorById('openrouter').models));
    setContextWindow(vendorById('openrouter').defaultContextWindow ?? 'auto');
    setEndpoints(defaultCreateRouteEndpoints('openrouter'));
    setEndpointUrls({});
    setError(null);
  };

  const applyVendor = (next: CreateRouteVendorId) => {
    const spec = vendorById(next);
    const autoNames = createRouteAutoNames(
      CREATE_ROUTE_VENDORS.map((item) => vendorLabel(t, item.id)),
    );
    setVendor(next);
    setName(nextCreateRouteName(
      name,
      defaultCreateRouteName(vendorLabel(t, next)),
      autoNames,
    ));
    if (next === 'custom') return;
    setUrl(spec.url);
    setEndpoints([...spec.enabled]);
    setEndpointUrls({});
    setModels(formatCreateRouteModels(spec.models));
    setContextWindow(spec.defaultContextWindow ?? 'auto');
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
    <DialogOrSide
      asPanel={asPanel}
      width={width}
      open={open}
      onOpenChange={(next) => {
        if (busy) return;
        if (!next) reset();
        onOpenChange(next);
      }}
      title={t('routes.create.title')}
      description={t('routes.create.description')}
      preventDismiss
      primary={(
        <Button type="submit" form="create-route-form" disabled={busy || !canSubmit} size="sm">
          {busy ? t('routes.create.submitting') : t('routes.create.submit')}
        </Button>
      )}
    >
        <form
          id="create-route-form"
          className="flex min-h-0 flex-1 flex-col space-y-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (busy || !canSubmit) return;
            void submitCreate();
          }}
        >
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
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.create.contextWindow')}</span>
              <select
                className="h-9 rounded-btn border border-border bg-background px-2 text-sm"
                value={contextWindow}
                onChange={(event) => setContextWindow(event.target.value as ClaudeContextWindowChoice)}
              >
                <option value="auto">{t('routes.create.contextWindowAuto')}</option>
                <option value="200000">{t('routes.create.contextWindow200k')}</option>
                <option value="1048576">{t('routes.create.contextWindow1m')}</option>
              </select>
              <p className="text-meta text-muted">{t('routes.create.contextWindowHint')}</p>
            </label>
            <fieldset className="space-y-2">
              <legend className="text-xs text-muted">{t('routes.create.upstreamEndpoints')}</legend>
              {CREATE_ROUTE_TARGETS.map((target) => {
                const checked = endpoints.includes(target);
                const targetUrl = endpointUrlFor(vendor, target, url, endpointUrls);
                return (
                  <div key={target} className="space-y-1.5 rounded-card border border-border bg-subtle/40 p-2">
                    <label className="flex items-start gap-2 text-sm">
                      <input
                        type="checkbox"
                        className="mt-0.5"
                        checked={checked}
                        onChange={() => toggleEndpoint(target)}
                      />
                      <span className="min-w-0">
                        <span className="block font-medium">{targetLabel(t, target)}</span>
                      </span>
                    </label>
                    {checked && vendor === 'custom' ? (
                      <label className="flex flex-col gap-1 pl-6">
                        <span className="text-meta text-muted">{t('routes.create.upstreamUrlFor', { target: targetLabel(t, target) })}</span>
                        <Input
                          value={endpointUrls[target] ?? url}
                          onChange={(event) => {
                            setEndpointUrls((current) => ({ ...current, [target]: event.target.value }));
                          }}
                          autoComplete="off"
                          spellCheck={false}
                          placeholder={url || 'https://'}
                        />
                      </label>
                    ) : checked ? (
                      <p className="break-all pl-6 text-meta text-muted">{targetUrl}</p>
                    ) : null}
                  </div>
                );
              })}
              <p className="text-meta text-muted">{t('routes.create.upstreamEndpointsHint')}</p>
            </fieldset>
            {error ? <p className="text-sm text-danger">{error}</p> : null}
        </form>
    </DialogOrSide>
  );
}
