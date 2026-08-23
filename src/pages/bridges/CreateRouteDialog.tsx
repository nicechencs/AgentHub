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
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  canSubmitCreateRoute,
  CREATE_ROUTE_TARGETS,
  CREATE_ROUTE_VENDORS,
  DEFAULT_CREATE_ROUTE_MODEL,
  defaultCreateRouteEndpoints,
  formatCreateRouteModels,
  isCreateRouteUrlValid,
  submitCreateRoute,
  submitImportRoute,
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
  entries = [],
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
  entries?: readonly ConnectionEntry[];
}) {
  const { t } = useI18n();
  const [mode, setMode] = useState<'create' | 'import'>('create');
  const [vendor, setVendor] = useState<CreateRouteVendorId>('openrouter');
  const [name, setName] = useState('');
  const [url, setUrl] = useState(vendorById('openrouter').url);
  const [key, setKey] = useState('');
  const [models, setModels] = useState(formatCreateRouteModels(vendorById('openrouter').models));
  const [endpoints, setEndpoints] = useState<CreateRouteTarget[]>(defaultCreateRouteEndpoints('openrouter'));
  const [importKey, setImportKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = () => {
    setMode('create');
    setVendor('openrouter');
    setName('');
    setUrl(vendorById('openrouter').url);
    setKey('');
    setModels(formatCreateRouteModels(vendorById('openrouter').models));
    setEndpoints(defaultCreateRouteEndpoints('openrouter'));
    setImportKey('');
    setError(null);
  };

  const applyVendor = (next: CreateRouteVendorId) => {
    const spec = vendorById(next);
    setVendor(next);
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
    const input = { name, url, key, vendor, endpoints, models };
    if (!canSubmitCreateRoute(input)) {
      setError(url.trim() && !isCreateRouteUrlValid(url)
        ? t('routes.create.urlInvalid')
        : t('routes.create.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitCreateRoute({
        ...input,
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

  const submitImport = async () => {
    const entry = entries.find((item) => item.key === importKey);
    if (!entry) {
      setError(t('routes.import.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitImportRoute({
        sourceKind: entry.source,
        sourceId: entry.id,
        agentId: entry.agentId,
      });
      reset();
      onOpenChange(false);
      onCreated();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t('routes.import.fallback'));
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
        className="flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden"
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
        onFocusOutside={(event) => event.preventDefault()}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>
            {mode === 'import' ? t('routes.import.title') : t('routes.create.title')}
          </DialogTitle>
          <DialogDescription>
            {mode === 'import' ? t('routes.import.description') : t('routes.create.description')}
          </DialogDescription>
        </DialogHeader>
        <div className="flex shrink-0 gap-2">
          <Button
            type="button"
            size="sm"
            variant={mode === 'create' ? 'default' : 'secondary'}
            disabled={busy}
            onClick={() => { setMode('create'); setError(null); }}
          >
            {t('routes.create.action')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant={mode === 'import' ? 'default' : 'secondary'}
            disabled={busy}
            onClick={() => { setMode('import'); setError(null); }}
          >
            {t('routes.import.action')}
          </Button>
        </div>
        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
          {mode === 'create' ? (
            <>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">{t('routes.create.vendorLabel')}</span>
                <select
                  className="h-9 rounded-md border border-border bg-background px-2 text-sm"
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
                  <label key={target} className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={endpoints.includes(target)}
                      onChange={() => toggleEndpoint(target)}
                    />
                    {targetLabel(t, target)}
                  </label>
                ))}
                <p className="text-meta text-muted">{t('routes.create.targetsHint')}</p>
              </fieldset>
            </>
          ) : entries.length === 0 ? (
            <p className="text-sm text-secondary">{t('routes.import.empty')}</p>
          ) : (
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">{t('routes.import.title')}</span>
              <select
                className="h-9 rounded-md border border-border bg-background px-2 text-sm"
                value={importKey}
                onChange={(event) => setImportKey(event.target.value)}
              >
                <option value="">{t('routes.import.required')}</option>
                {entries.map((entry) => (
                  <option key={entry.key} value={entry.key}>
                    {entry.title}
                  </option>
                ))}
              </select>
            </label>
          )}
          {error ? <p className="text-sm text-danger">{error}</p> : null}
        </div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button
            onClick={() => { void (mode === 'import' ? submitImport() : submitCreate()); }}
            disabled={busy}
          >
            {busy
              ? (mode === 'import' ? t('routes.import.submitting') : t('routes.create.submitting'))
              : (mode === 'import' ? t('routes.import.submit') : t('routes.create.submit'))}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
