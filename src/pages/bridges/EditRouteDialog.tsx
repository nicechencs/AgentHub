import { useEffect, useMemo, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SecretInput } from '@/components/shared/SecretInput';
import { Button } from '@/components/ui/button';
import { DialogOrSide } from './dialog-or-side';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';
import type { Provider } from '@/lib/types';
import {
  CREATE_ROUTE_TARGETS,
  canSubmitEditRoute,
  editRouteFormFromProvider,
  endpointUrlFor,
  isCreateRouteUrlValid,
  isEditableRouteSource,
  readStoredCreateRouteVendor,
  submitEditRoute,
  type CreateRouteTarget,
  type EditRouteInput,
} from './create-route-flow';

function targetLabel(t: TranslateFn, target: CreateRouteTarget): string {
  if (target === 'claude') return t('routes.create.target.claude');
  if (target === 'codex') return t('routes.create.target.codex');
  return t('routes.create.target.grok');
}

function seedEditForm(provider: Provider | null): EditRouteInput {
  if (!provider) return { name: '', url: '', key: '', endpoints: [], models: '', endpointUrls: {} };
  return editRouteFormFromProvider(provider);
}

export function EditRouteDialog({
  open,
  onOpenChange,
  profile,
  entries,
  busy,
  onSaved,
  onRequestDelete,
  asPanel = false,
  width,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  profile: AdapterProfile | null;
  entries: readonly ConnectionEntry[];
  /** Page-level mutation in flight — disables the destructive action. */
  busy?: boolean;
  onSaved: () => void;
  /** Hands the delete/停止并还原 confirmation back to the page. */
  onRequestDelete: (profile: AdapterProfile) => void;
  asPanel?: boolean;
  width?: number;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const sourceEntry = profile
    ? entries.find((entry) => entry.source === profile.sourceKind && entry.id === profile.sourceId)
    : undefined;
  const sourceProvider = sourceEntry?.provider ?? null;
  const editable = profile
    ? isEditableRouteSource({ sourceKind: profile.sourceKind, provider: sourceProvider })
    : false;
  const profileId = profile?.id ?? null;
  const seed = useMemo(
    () => seedEditForm(sourceProvider),
    [sourceProvider?.name, sourceProvider?.configText],
  );

  const [name, setName] = useState(seed.name);
  const [url, setUrl] = useState(seed.url);
  const [key, setKey] = useState('');
  const [models, setModels] = useState(seed.models ?? '');
  const [endpoints, setEndpoints] = useState<CreateRouteTarget[]>([...seed.endpoints]);
  const [endpointUrls, setEndpointUrls] = useState<Partial<Record<CreateRouteTarget, string>>>({ ...seed.endpointUrls });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName(seed.name);
    setUrl(seed.url);
    setKey('');
    setModels(seed.models ?? '');
    setEndpoints([...seed.endpoints]);
    setEndpointUrls({ ...seed.endpointUrls });
    setError(null);
    // Re-seeding on every entries refresh would clobber typing mid-edit.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, profileId]);

  if (!profile) return null;

  const storedVendor = readStoredCreateRouteVendor(sourceProvider?.configText);
  const editInput = { name, url, key, endpoints, models, endpointUrls };
  const canSubmit = editable && canSubmitEditRoute(editInput);

  const toggleEndpoint = (target: CreateRouteTarget) => {
    setEndpoints((current) =>
      current.includes(target)
        ? current.filter((item) => item !== target)
        : [...current, target],
    );
  };

  const requestDelete = () => {
    if (submitting) return;
    onOpenChange(false);
    onRequestDelete(profile);
  };

  const save = async () => {
    if (submitting || !sourceProvider) return;
    if (!canSubmitEditRoute(editInput)) {
      setError(url.trim() && !isCreateRouteUrlValid(url)
        ? t('routes.create.urlInvalid')
        : t('routes.create.required'));
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await submitEditRoute(sourceProvider, editInput);
      onOpenChange(false);
      onSaved();
      toast({ title: t('routes.edit.success'), variant: 'success' });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t('routes.edit.fallback'));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <DialogOrSide
      asPanel={asPanel}
      width={width}
      open={open}
      onOpenChange={(next) => {
        if (submitting) return;
        if (!next) setError(null);
        onOpenChange(next);
      }}
      title={t('routes.edit.title')}
      description={t('routes.edit.description')}
      preventDismiss
      primary={editable ? (
        <Button type="submit" form="edit-route-form" disabled={submitting || !canSubmit} size="sm">
          {submitting ? t('routes.edit.submitting') : t('routes.edit.submit')}
        </Button>
      ) : undefined}
      danger={(
        <Button
          type="button"
          size="sm"
          variant="dangerOutline"
          onClick={requestDelete}
          disabled={busy || submitting}
        >
          {t('routes.delete.action')}
        </Button>
      )}
    >
        <form
          id="edit-route-form"
          className="flex min-h-0 flex-1 flex-col space-y-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (submitting || !canSubmit) return;
            void save();
          }}
        >
            {editable ? (
              <>
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
                  <SecretInput
                    value={key}
                    onChange={setKey}
                    placeholder={t('routes.edit.keyPlaceholder')}
                  />
                  <p className="text-meta text-muted">{t('routes.edit.keyHint')}</p>
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
                  <legend className="text-xs text-muted">{t('routes.create.upstreamEndpoints')}</legend>
                  {CREATE_ROUTE_TARGETS.map((target) => {
                    const checked = endpoints.includes(target);
                    const targetUrl = endpointUrlFor(storedVendor, target, url, endpointUrls);
                    const canEditUrl = storedVendor === 'custom';
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
                        {checked && canEditUrl ? (
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
                  <p className="text-meta text-muted">{t('routes.edit.endpointsHint')}</p>
                </fieldset>
              </>
            ) : (
              <p className="text-sm text-secondary">{t('routes.edit.unavailable')}</p>
            )}
            {error ? <p className="text-sm text-danger">{error}</p> : null}
        </form>
    </DialogOrSide>
  );
}
