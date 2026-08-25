import { useMemo, useState, type MouseEvent } from 'react';
import { Link } from 'react-router-dom';
import { DetailsToggle } from '@/components/shared/DetailsToggle';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { DialogOrSide } from './dialog-or-side';
import { agentDisplayName } from '@/config/agents';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { AuthStatus } from '@/lib/types';
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
  asPanel = false,
  width,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entries: readonly ConnectionEntry[];
  profiles?: readonly (Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'route' | 'generatedProviderId'> & { name?: string })[];
  bindingProfileIds?: ReadonlySet<string>;
  onImported: () => void;
  asPanel?: boolean;
  width?: number;
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
      profiles ?? [],
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
    <DialogOrSide
      asPanel={asPanel}
      width={width}
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
      title={t('routes.import.title')}
      description={t('routes.import.description')}
      preventDismiss
      primary={(
        <Button type="submit" form="import-route-form" disabled={busy || !picked} size="sm">
          {busy ? t('routes.import.submitting') : t('routes.import.submit')}
        </Button>
      )}
    >
      <form
        id="import-route-form"
        className="flex min-h-0 flex-1 flex-col space-y-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (busy || !picked) return;
          void submit();
        }}
      >
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
                      <DetailsToggle
                        open={openRow}
                        controlsId={detailsId}
                        onClick={(event) => {
                          stopRadioSelect(event);
                          toggleExpanded(entry.key);
                        }}
                        onMouseDown={stopRadioSelect}
                      >
                        {t('connections.list.details')}
                      </DetailsToggle>
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
      </form>
    </DialogOrSide>
  );
}
