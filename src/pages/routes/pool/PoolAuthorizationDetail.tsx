import { useEffect, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { DetailRow } from '@/components/shared/DetailRow';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { ensureSourceModelCatalog } from '@/lib/api/adapter';
import type { AccountAction } from '@/lib/backend/contracts/account-actions';
import type { SourceModelCatalog } from '@/lib/backend/contracts/adapter';
import { connectionKindLabel } from '@/lib/connection-kind';
import { localEndpointPath } from '@/lib/route-endpoints';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import { adapterStatusTextClass } from '@/pages/routes/shared/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/routes/shared/route-pool-view-model';
import { ApiAccessForm } from './ApiAccessDialog';
import type { PoolApiEditTarget } from './api-access-model';
import { PoolEndpointTypeLine } from './PoolEndpointTypeLine';
import { PoolLoginMark } from './PoolLoginMark';
import {
  hasQuotaWindow,
  poolAuthorizationDetailRows,
  poolAuthorizationEndpointKinds,
  poolAuthorizationLoginLabel,
  poolAuthorizationTypeHref,
} from './pool-authorization-detail';
import { poolAuthorizationRefreshLabels } from './pool-authorization-refresh';

export function PoolAuthorizationDetail({
  item,
  width,
  toggling,
  refreshing,
  oauthAction,
  agents = [],
  editTarget,
  onEnabledChange,
  onRefresh,
  onDelete,
  onSaved,
  onClose,
}: {
  item: PoolAuthorizationItem;
  width?: number;
  toggling?: boolean;
  refreshing?: boolean;
  oauthAction?: AccountAction;
  agents?: readonly AgentKey[];
  editTarget?: PoolApiEditTarget | null;
  onEnabledChange?: (enabled: boolean) => void;
  onRefresh?: () => void;
  onDelete: () => void;
  onSaved?: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const status = poolAuthorizationStatusView(item, t);
  const rows = poolAuthorizationDetailRows(item, t);
  const endpointKinds = poolAuthorizationEndpointKinds(item);
  const fieldRows = rows.filter((row) => row.id !== 'endpointTypes');
  const displayTitle = poolAuthorizationLoginLabel(item);
  const hasQuota = hasQuotaWindow(item.quota7dPct) || hasQuotaWindow(item.quota5hPct);
  const canEditKey = Boolean(editTarget?.provider.id) && item.kind === 'apikey';
  const editLabel = canEditKey ? t('connections.list.editKey') : null;
  const refreshLabels = oauthAction ? poolAuthorizationRefreshLabels(oauthAction, t) : null;
  const [editing, setEditing] = useState(false);
  const [catalog, setCatalog] = useState<SourceModelCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogFailed, setCatalogFailed] = useState(false);

  useEffect(() => {
    setEditing(false);
  }, [item.key]);

  useEffect(() => {
    let cancelled = false;
    setCatalog(null);
    setCatalogFailed(false);
    setCatalogLoading(true);
    void ensureSourceModelCatalog(item.sourceKind, item.sourceId)
      .then((next) => {
        if (cancelled) return;
        setCatalog(next);
      })
      .catch(() => {
        if (!cancelled) setCatalogFailed(true);
      })
      .finally(() => {
        if (!cancelled) setCatalogLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [item.sourceKind, item.sourceId]);

  return (
    <SideInspectPanel
      title={editing ? t('routes.pool.page.apiDialogEditTitle') : t('routes.pool.detail.title')}
      description={displayTitle}
      onClose={onClose}
      width={width}
      headerActions={editing ? undefined : (
        <>
          {refreshLabels && onRefresh ? (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={refreshing}
              aria-label={refreshLabels.idle}
              onClick={onRefresh}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', refreshing && 'animate-spin')} />
              {refreshing ? refreshLabels.busy : refreshLabels.idle}
            </Button>
          ) : null}
          <Button size="sm" variant="dangerOutline" onClick={onDelete}>
            <Trash2 className="h-3.5 w-3.5" /> {t('connections.list.moveToTrash')}
          </Button>
          {editLabel ? (
            <Button type="button" size="sm" variant="outline" onClick={() => setEditing(true)}>
              {editLabel}
            </Button>
          ) : null}
        </>
      )}
    >
      {editing && editTarget ? (
        <ApiAccessForm
          layout="inline"
          agents={agents}
          edit={editTarget}
          onCancel={() => setEditing(false)}
          onSaved={() => {
            setEditing(false);
            onSaved?.();
          }}
        />
      ) : (
      <div className="flex flex-col gap-3 text-xs" data-pool-authorization-detail={item.key}>
        <div className="flex min-w-0 flex-wrap items-center gap-2 text-sm">
          <PoolLoginMark item={item} />
          <span className="truncate font-medium text-primary">{displayTitle}</span>
          <span className="text-meta text-muted">{connectionKindLabel(item.kind, t)}</span>
          <span className={adapterStatusTextClass(status.tone)}>{status.label}</span>
        </div>

        {item.canToggle ? (
          <label className="flex items-center justify-between gap-3 rounded-card border border-border px-3 py-2">
            <span className="text-sm text-primary">{t('routes.pool.detail.enabled')}</span>
            <Switch
              checked={item.enabled !== false}
              disabled={toggling}
              onCheckedChange={onEnabledChange}
              aria-label={t('routes.pool.detail.enabled')}
            />
          </label>
        ) : null}

        {hasQuota ? (
          <div>
            <p className="text-meta text-muted">{t('routes.pool.detail.quota')}</p>
            <div className="mt-1.5 flex flex-col gap-1.5">
              {hasQuotaWindow(item.quota7dPct) ? (
                <QuotaBar label="7d" pct={item.quota7dPct} resetIn={item.quota7dResetIn} />
              ) : null}
              {hasQuotaWindow(item.quota5hPct) ? (
                <QuotaBar label="5h" pct={item.quota5hPct} resetIn={item.quotaResetIn} />
              ) : null}
            </div>
          </div>
        ) : null}

        {endpointKinds.length > 0 || fieldRows.length > 0 ? (
          <div className="grid gap-1.5 text-secondary sm:grid-cols-1">
            {endpointKinds.length > 0 ? (
              <span className="flex min-w-0 items-start gap-1.5">
                <span className="min-w-0 flex-1">
                  <span className="text-muted">{t('routes.pool.detail.endpointTypes')} </span>
                  <span className="inline-flex flex-col gap-0.5 align-top">
                    {endpointKinds.map((kind) => (
                      <PoolEndpointTypeLine
                        key={kind}
                        kind={kind}
                        href={poolAuthorizationTypeHref(item.endpointHost, localEndpointPath(kind)) ?? undefined}
                      />
                    ))}
                  </span>
                </span>
              </span>
            ) : null}
            {fieldRows.map((row) => (
              <DetailRow
                key={row.id}
                label={row.label}
                value={row.value}
                lines={row.lines}
                href={row.href}
                mono={row.mono}
                copyable={row.copyable}
              />
            ))}
          </div>
        ) : null}

        <div className="flex flex-col gap-1.5">
          <p className="text-meta text-muted">{t('routes.pool.detail.models')}</p>
          {catalogLoading ? (
            <p className="text-meta text-secondary">…</p>
          ) : catalogFailed ? (
            <p className="text-meta text-secondary">{t('routes.pool.detail.modelsLoadFailed')}</p>
          ) : catalog && catalog.models.length > 0 ? (
            <p className="text-sm text-primary">
              {catalog.models.join(', ')}
              <span className="ml-2 text-meta text-muted">
                {catalog.source === 'custom'
                  ? t('routes.pool.detail.modelsCustom')
                  : t('routes.pool.detail.modelsLive')}
              </span>
            </p>
          ) : (
            <p className="text-meta text-secondary">{t('routes.pool.detail.modelsEmpty')}</p>
          )}
        </div>
      </div>
      )}
    </SideInspectPanel>
  );
}
