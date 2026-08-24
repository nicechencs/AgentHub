import { useState } from 'react';
import { Link } from 'react-router-dom';
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
import type { ConnectionEntry } from '@/lib/connection-entry';
import { submitImportRoute } from './create-route-flow';

export function ImportRouteDialog({
  open,
  onOpenChange,
  entries,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entries: readonly ConnectionEntry[];
  onImported: () => void;
}) {
  const { t } = useI18n();
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const picked = entries.find((entry) => entry.key === selected) ?? null;

  const submit = async () => {
    if (busy) return;
    if (!picked) {
      setError(t('routes.import.required'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submitImportRoute({
        sourceKind: picked.source,
        sourceId: picked.id,
        agentId: picked.agentId,
      });
      setSelected(null);
      onOpenChange(false);
      onImported();
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
        if (!next) {
          setSelected(null);
          setError(null);
        }
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
          <DialogTitle>{t('routes.import.title')}</DialogTitle>
          <DialogDescription>{t('routes.import.description')}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
          {entries.length === 0 ? (
            <p className="text-sm text-muted">
              {t('routes.import.empty')}{' '}
              <Link to="/connections" className="text-accent underline-offset-2 hover:underline">
                {t('nav.connections')}
              </Link>
            </p>
          ) : (
            <ul className="space-y-1">
              {entries.map((entry) => (
                <li key={entry.key}>
                  <label className="flex cursor-pointer items-start gap-2 rounded-card border border-border p-2 text-sm">
                    <input
                      type="radio"
                      name="import-login"
                      className="mt-0.5"
                      checked={selected === entry.key}
                      onChange={() => setSelected(entry.key)}
                    />
                    <span className="min-w-0">
                      <span className="block truncate">{entry.title}</span>
                      <span className="block text-meta text-muted">{entry.subtitle}</span>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
          {error ? <p className="text-sm text-danger">{error}</p> : null}
        </div>
        <DialogFooter className="mt-4 shrink-0 border-t border-border pt-4">
          <Button type="button" variant="secondary" onClick={() => onOpenChange(false)} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button type="button" onClick={() => void submit()} disabled={busy || !picked}>
            {busy ? t('routes.import.submitting') : t('routes.import.submit')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
