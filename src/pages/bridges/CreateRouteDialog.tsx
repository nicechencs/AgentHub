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
  CREATE_ROUTE_TARGETS,
  DEFAULT_CREATE_ROUTE_MODEL,
  submitCreateRoute,
  type CreateRouteTarget,
} from './create-route-flow';

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
  const [url, setUrl] = useState('https://openrouter.ai/api/v1');
  const [key, setKey] = useState('');
  const [model, setModel] = useState(DEFAULT_CREATE_ROUTE_MODEL);
  const [targets, setTargets] = useState<CreateRouteTarget[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reset = () => {
    setName('');
    setUrl('https://openrouter.ai/api/v1');
    setKey('');
    setModel(DEFAULT_CREATE_ROUTE_MODEL);
    setTargets([]);
    setError(null);
  };

  const toggleTarget = (target: CreateRouteTarget) => {
    setTargets((current) =>
      current.includes(target)
        ? current.filter((item) => item !== target)
        : [...current, target],
    );
  };

  const submit = async () => {
    if (!canSubmitCreateRoute({ name, url, key, targets, model })) {
      setError(url.trim() && !url.trim().startsWith('http')
        ? t('routes.create.urlInvalid')
        : t('routes.create.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitCreateRoute({
        name,
        url,
        key,
        targets,
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
            <span className="text-xs text-muted">{t('routes.create.url')}</span>
            <Input value={url} onChange={(event) => setUrl(event.target.value)} autoComplete="off" spellCheck={false} />
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
            {CREATE_ROUTE_TARGETS.map((target) => (
              <label key={target} className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={targets.includes(target)}
                  onChange={() => toggleTarget(target)}
                />
                {target === 'claude'
                  ? t('routes.create.target.claude')
                  : target === 'codex'
                    ? t('routes.create.target.codex')
                    : t('routes.create.target.grok')}
              </label>
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
