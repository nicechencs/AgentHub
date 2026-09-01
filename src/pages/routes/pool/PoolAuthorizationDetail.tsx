import { useEffect, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { agentDisplayName } from '@/config/agents';
import { AgentDot } from '@/components/shared/AgentDot';
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
import { cn } from '@/lib/utils';
import { adapterStatusTextClass } from '@/pages/bridges/adapter-view-model';
import {
  poolAuthorizationStatusView,
  type PoolAuthorizationItem,
} from '@/pages/bridges/route-pool-view-model';
import {
  hasQuotaWindow,
  poolAuthorizationDetailRows,
  poolAuthorizationLoginLabel,
} from './pool-authorization-detail';
import { poolAuthorizationRefreshLabels } from './pool-authorization-refresh';

export function PoolAuthorizationDetail({
  item,
  width,
  toggling,
  refreshing,
  oauthAction,
  onEnabledChange,
  onRefresh,
  onDelete,
  onEdit,
  onClose,
}: {
  item: PoolAuthorizationItem;
  width?: number;
  toggling?: boolean;
  refreshing?: boolean;
  oauthAction?: AccountAction;
  onEnabledChange?: (enabled: boolean) => void;
  onRefresh?: () => void;
  onDelete: () => void;
  onEdit?: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const status = poolAuthorizationStatusView(item, t);
  const rows = poolAuthorizationDetailRows(item, t);
  const displayTitle = poolAuthorizationLoginLabel(item);
  const hasQuota = hasQuotaWindow(item.quota7dPct) || hasQuotaWindow(item.quota5hPct);
  const editLabel = onEdit
    ? (item.kind === 'apikey' ? t('connections.list.editKey') : null)
    : null;
  const refreshLabels = oauthAction ? poolAuthorizationRefreshLabels(oauthAction, t) : null;
  const [catalog, setCatalog] = useState<SourceModelCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogFailed, setCatalogFailed] = useState(false);

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
      title={t('routes.pool.detail.title')}
      description={displayTitle}
      onClose={onClose}
      width={width}
      headerActions={(
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
            <Button type="button" size="sm" variant="outline" onClick={onEdit}>
              {editLabel}
            </Button>
          ) : null}
        </>
      )}
    >
      <div className="flex flex-col gap-3 text-xs" data-pool-authorization-detail={item.key}>
        <div className="flex min-w-0 flex-wrap items-center gap-2 text-sm">
          <AgentDot agentId={item.agentId} size="sm" title={null} />
          <span className="truncate font-medium text-primary">{displayTitle}</span>
          <span className="text-meta text-muted">{connectionKindLabel(item.kind, t)}</span>
          <span className={adapterStatusTextClass(status.tone)}>{status.label}</span>
        </div>
        <p className="text-meta text-secondary">{agentDisplayName(item.agentId)}</p>

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

        {rows.length > 0 ? (
          <div className="grid gap-1.5 text-secondary sm:grid-cols-1">
            {rows.map((row) => (
              <DetailRow
                key={row.id}
                label={row.label}
                value={row.value}
                lines={row.lines}
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
    </SideInspectPanel>
  );
}
