import { useMemo, useState, type MouseEvent } from 'react';
import { Link } from 'react-router-dom';
import { ChevronDown } from 'lucide-react';
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
import { agentDisplayName } from '@/config/agents';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { AuthStatus } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  alreadyRoutedSourceKeys,
  importableConnectionEntries,
  importRouteRowTitle,
  submitImportRoute,
} from './create-route-flow';

const AUTH_STATUS_KEY: Record<AuthStatus, MessageKey> = {
  valid: 'chrome.authStatus.valid',
  expiring: 'chrome.authStatus.expiring',
  expired: 'chrome.authStatus.expired',
  none: 'chrome.authStatus.none',
};

function importRowAgentLabel(t: TranslateFn, agentId: ConnectionEntry['agentId']): string {
  if (agentId === 'claude') return t('routes.create.target.claude');
  if (agentId === 'codex') return t('routes.create.target.codex');
  if (agentId === 'grok') return t('routes.create.target.grok');
  return agentDisplayName(agentId);
}

function stopRadioSelect(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
}

export function ImportRouteDialog({
  open,
  onOpenChange,
  entries,
  profiles,
  bindingProfileIds,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entries: readonly ConnectionEntry[];
  profiles?: readonly Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'route' | 'generatedProviderId'>[];
  bindingProfileIds?: ReadonlySet<string>;
  onImported: () => void;
}) {
  const { t } = useI18n();
  const [selected, setSelected] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const importable = useMemo(
    () => importableConnectionEntries(
      entries,
      alreadyRoutedSourceKeys(profiles ?? [], bindingProfileIds),
    ),
    [entries, profiles, bindingProfileIds],
  );

  const picked = importable.find((entry) => entry.key === selected) ?? null;

  const toggleExpanded = (key: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

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
      setExpanded(new Set());
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
          setExpanded(new Set());
          setError(null);
        }
        onOpenChange(next);
      }}
    >
      <DialogContent
        className="flex max-h-[min(36rem,calc(100vh-2rem))] flex-col overflow-hidden"
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
        onFocusOutside={(event) => event.preventDefault()}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>{t('routes.import.title')}</DialogTitle>
          <DialogDescription>{t('routes.import.description')}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
          {importable.length === 0 ? (
            <p className="text-sm text-muted">
              {t('routes.import.empty')}{' '}
              <Link to="/connections" className="text-accent underline-offset-2 hover:underline">
                {t('nav.connections')}
              </Link>
            </p>
          ) : (
            <ul className="space-y-1">
              {importable.map((entry) => {
                const openRow = expanded.has(entry.key);
                const detailsId = `import-login-details-${entry.key}`;
                const agent = importRowAgentLabel(t, entry.agentId);
                const endpointLabel = entry.endpointMode === 'custom'
                  ? t('connections.list.customEndpoint')
                  : entry.endpointMode === 'official'
                    ? t('connections.list.officialEndpoint')
                    : null;
                return (
                  <li key={entry.key} className="rounded-card border border-border p-2 text-sm">
                    <div className="flex items-start gap-2">
                      <label className="flex min-w-0 flex-1 cursor-pointer items-start gap-2">
                        <input
                          type="radio"
                          name="import-login"
                          className="mt-0.5"
                          checked={selected === entry.key}
                          onChange={() => setSelected(entry.key)}
                        />
                        <span className="min-w-0 truncate">
                          {importRouteRowTitle(entry, {
                            agent,
                            officialEndpoint: t('connections.list.officialEndpoint'),
                            customEndpoint: t('connections.list.customEndpoint'),
                          })}
                        </span>
                      </label>
                      <button
                        type="button"
                        className="inline-flex shrink-0 items-center gap-1 text-meta text-muted"
                        aria-expanded={openRow}
                        aria-controls={detailsId}
                        onClick={(event) => {
                          stopRadioSelect(event);
                          toggleExpanded(entry.key);
                        }}
                        onMouseDown={stopRadioSelect}
                      >
                        {t('connections.list.details')}
                        <ChevronDown
                          className={cn('h-3.5 w-3.5 transition-transform', openRow && 'rotate-180')}
                          aria-hidden
                        />
                      </button>
                    </div>
                    {openRow ? (
                      <div id={detailsId} className="mt-2 space-y-1 pl-6 text-meta text-muted">
                        {entry.subtitle ? <p>{entry.subtitle}</p> : null}
                        <p>{agent}</p>
                        {endpointLabel ? <p>{endpointLabel}</p> : null}
                        {entry.endpointHost ? <p>{entry.endpointHost}</p> : null}
                        <p>{t(AUTH_STATUS_KEY[entry.authStatus])}</p>
                        {entry.identityLabel ? <p>{entry.identityLabel}</p> : null}
                      </div>
                    ) : null}
                  </li>
                );
              })}
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
