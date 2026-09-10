import { useEffect, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { DetailRow } from '@/components/shared/DetailRow';
import {
  DetailTable,
  DetailTableCell,
  DetailTableRow,
} from '@/components/shared/DetailTable';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/tooltip';
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
import { OauthLoginEditForm } from './OauthLoginEditForm';
import {
  poolAuthorizationOauthEditable,
  type SaveOauthPoolLoginResult,
} from './pool-authorization-edit';
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
import { PoolAuthorizationSyncPrompt } from './PoolAuthorizationSyncPrompt';

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
  onSaved?: (nextKey?: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const status = poolAuthorizationStatusView(item, t);
  const rows = poolAuthorizationDetailRows(item, t);
  const endpointKinds = poolAuthorizationEndpointKinds(item);
  const fieldRows = rows.filter((row) => row.id !== 'endpointTypes');
  const whereRows = fieldRows.filter((row) => row.id === 'subscription' || row.id === 'endpoint');
  const recordRows = fieldRows.filter((row) => row.id !== 'subscription' && row.id !== 'endpoint');
  const displayTitle = poolAuthorizationLoginLabel(item);
  const hasQuota = hasQuotaWindow(item.quota7dPct) || hasQuotaWindow(item.quota5hPct);
  const canEditKey = Boolean(editTarget?.provider.id) && item.kind === 'apikey';
  const canEditOauth = poolAuthorizationOauthEditable(item);
  const editLabel = canEditKey
    ? t('connections.list.editKey')
    : canEditOauth
      ? t('routes.pool.page.editLogin')
      : null;
  const refreshLabels = oauthAction ? poolAuthorizationRefreshLabels(oauthAction, t) : null;
  const [editing, setEditing] = useState(false);
  const [catalog, setCatalog] = useState<SourceModelCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogFailed, setCatalogFailed] = useState(false);
  const [syncPrompt, setSyncPrompt] = useState<SaveOauthPoolLoginResult | null>(null);
  const [modelsOpen, setModelsOpen] = useState(false);


  const finishOauthSave = (result: SaveOauthPoolLoginResult) => {
    setSyncPrompt(null);
    setEditing(false);
    onSaved?.(result.copied ? `${result.sourceKind}:${result.sourceId}` : undefined);
  };

  useEffect(() => {
    setEditing(false);
    setSyncPrompt(null);
    setModelsOpen(false);
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
    <>
    <SideInspectPanel
      title={editing
        ? (canEditOauth ? t('routes.pool.page.editLoginTitle') : t('routes.pool.page.apiDialogEditTitle'))
        : t('routes.pool.detail.title')}
      description={displayTitle}
      onClose={onClose}
      width={width}
      headerActions={editing ? undefined : (
        <>
          {refreshLabels && onRefresh ? (
            <Hint label={refreshing ? refreshLabels.busy : refreshLabels.tip}>
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
            </Hint>
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
      {editing && canEditOauth ? (
        <OauthLoginEditForm
          item={item}
          catalog={catalog}
          onCancel={() => setEditing(false)}
          onSaved={(result) => {
            if (result.copied) {
              setSyncPrompt(result);
              return;
            }
            finishOauthSave(result);
          }}
        />
      ) : editing && editTarget ? (
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
      <div className="flex flex-col gap-3" data-pool-authorization-detail={item.key}>
        <section className="space-y-1.5">
          <h3 className="text-body font-medium">{t('routes.pool.detail.sectionAvailability')}</h3>
          <div className="flex min-w-0 flex-wrap items-center gap-2 text-body">
            <PoolLoginMark item={item} />
            <span className="truncate font-medium text-primary">{displayTitle}</span>
            <span className="text-meta text-muted">{connectionKindLabel(item.kind, t)}</span>
            <span className={adapterStatusTextClass(status.tone)}>{status.label}</span>
          </div>
          {item.canToggle ? (
            <label className="flex items-center justify-between gap-3 rounded-card border border-border px-3 py-2">
              <span className="text-body text-primary">{t('routes.pool.detail.enabled')}</span>
              <Switch
                checked={item.enabled !== false}
                disabled={toggling}
                onCheckedChange={onEnabledChange}
                aria-label={t('routes.pool.detail.enabled')}
              />
            </label>
          ) : null}
        </section>

        {hasQuota ? (
          <section className="space-y-1.5">
            <h3 className="text-body font-medium">{t('connections.list.usage')}</h3>
            <DetailTable>
              {hasQuotaWindow(item.quota7dPct) ? (
                <DetailTableRow label={t('connections.list.quota7dUsed')}>
                  <DetailTableCell>
                    <QuotaBar
                      label={t('connections.list.quota7dUsed')}
                      pct={item.quota7dPct}
                      compact
                      showLabel={false}
                    />
                  </DetailTableCell>
                  <DetailTableCell className="whitespace-nowrap text-meta text-muted">
                    {item.quota7dResetIn}
                  </DetailTableCell>
                </DetailTableRow>
              ) : null}
              {hasQuotaWindow(item.quota5hPct) ? (
                <DetailTableRow label={t('connections.list.quota5hUsed')}>
                  <DetailTableCell>
                    <QuotaBar
                      label={t('connections.list.quota5hUsed')}
                      pct={item.quota5hPct}
                      compact
                      showLabel={false}
                    />
                  </DetailTableCell>
                  <DetailTableCell className="whitespace-nowrap text-meta text-muted">
                    {item.quotaResetIn}
                  </DetailTableCell>
                </DetailTableRow>
              ) : null}
            </DetailTable>
          </section>
        ) : null}

        <section className="space-y-1.5">
          <h3 className="text-body font-medium">{t('connections.list.sectionWhere')}</h3>
          <DetailTable>
            {endpointKinds.length > 0 ? (
              <DetailTableRow label={t('routes.pool.detail.endpointTypes')}>
                <DetailTableCell>
                  <span className="inline-flex flex-col gap-0.5">
                    {endpointKinds.map((kind) => (
                      <PoolEndpointTypeLine
                        key={kind}
                        kind={kind}
                        href={poolAuthorizationTypeHref(item.endpointHost, localEndpointPath(kind)) ?? undefined}
                      />
                    ))}
                  </span>
                </DetailTableCell>
              </DetailTableRow>
            ) : null}
            {whereRows.map((row) => (
              <DetailRow
                key={row.id}
                label={row.label}
                value={row.value}
                lines={row.lines}
                href={row.href}
                mono={row.mono}
                copyable={row.copyable}
                className={row.copyable ? 'w-full' : undefined}
              />
            ))}
            <DetailTableRow label={t('routes.pool.detail.models')}>
              <DetailTableCell>
                {catalogLoading ? (
                  <p className="text-meta text-secondary">…</p>
                ) : catalogFailed ? (
                  <p className="text-meta text-secondary">{t('routes.pool.detail.modelsLoadFailed')}</p>
                ) : catalog && catalog.models.length > 0 ? (
                  <div className="flex flex-col gap-1.5">
                    <p className="text-body text-primary">
                      {(modelsOpen || catalog.models.length <= 8
                        ? catalog.models
                        : catalog.models.slice(0, 8)
                      ).join(', ')}
                      <span className="ml-2 text-meta text-muted">
                        {catalog.source === 'custom'
                          ? t('routes.pool.detail.modelsCustom')
                          : t('routes.pool.detail.modelsLive')}
                      </span>
                    </p>
                    {catalog.models.length > 8 ? (
                      <button
                        type="button"
                        className="self-start text-meta text-muted"
                        onClick={() => setModelsOpen((open) => !open)}
                      >
                        {modelsOpen
                          ? t('common.collapse')
                          : t('routes.pool.detail.modelsShowAll', { n: catalog.models.length })}
                      </button>
                    ) : null}
                  </div>
                ) : (
                  <p className="text-meta text-secondary">{t('routes.pool.detail.modelsEmpty')}</p>
                )}
              </DetailTableCell>
            </DetailTableRow>
          </DetailTable>
        </section>

        {recordRows.length > 0 ? (
          <section className="space-y-1.5">
            <h3 className="text-meta font-medium text-muted">{t('connections.list.sectionRecords')}</h3>
            <DetailTable>
              {recordRows.map((row) => (
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
            </DetailTable>
          </section>
        ) : null}
      </div>
      )}
    </SideInspectPanel>
    <PoolAuthorizationSyncPrompt prompt={syncPrompt} onFinish={finishOauthSave} />
    </>
  );
}
