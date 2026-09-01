import { useState } from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import { CopyableRouteEndpointUrl } from '@/components/shared/RouteEndpointUrl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { localEndpointBrandAgentId } from '@/lib/route-endpoints';
import {
  buildTokenDetailCopyRows,
  formatTokenRelative,
  tokenDetailTitle,
  tokenEndpointParts,
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import type { LocalTokenRow } from './tokens-model';

export function TokenDetailPanel({
  row,
  width,
  onClose,
  onEditKey,
}: {
  row: LocalTokenRow;
  width?: number;
  onClose: () => void;
  onEditKey?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [revealed, setRevealed] = useState(false);
  const copies = buildTokenDetailCopyRows(row, revealed, t);
  const endpoint = tokenEndpointParts(row);
  const typeRow = copies.find((item) => item.id === 'type');
  const tokenRow = copies.find((item) => item.id === 'token');
  const canCopyToken = Boolean(tokenRow?.copyValue);
  const copyToken = () => {
    const value = tokenRow?.copyValue;
    if (!value) return;
    void navigator.clipboard.writeText(value).then(
      () => toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };

  return (
    <SideInspectPanel
      title={t('routes.tokens.detailTitle')}
      description={tokenDetailTitle(row, t)}
      onClose={onClose}
      width={width}
    >
      <div className="flex flex-col gap-3 text-sm" data-token-detail={row.id}>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldType')}</p>
          <p className="text-primary">{typeRow?.display}</p>
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldEndpoint')}</p>
          {endpoint.portPending ? (
            <p className="font-mono text-muted">{t('routes.pendingPort')}</p>
          ) : (
            <CopyableRouteEndpointUrl
              path={row.path}
              port={endpoint.portPending ? null : Number(endpoint.portLabel)}
              host={endpoint.host}
              endpointId={endpoint.endpointId}
              brandAgentId={localEndpointBrandAgentId(row.kind)}
              className="text-sm"
            />
          )}
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldLastPage')}</p>
          <p className="font-mono text-secondary">{tokenLastPageDisplay(row) || '—'}</p>
          {row.lastRequestAt ? (
            <p className="text-meta text-muted">
              {t('routes.tokens.fieldLastAt')} · {formatTokenRelative(row.lastRequestAt, t)}
            </p>
          ) : null}
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldUsage')}</p>
          <p className="text-secondary">{tokenUsageDisplay(row.usage, t) || '—'}</p>
        </div>
        <div className="space-y-1">
          <p className="text-meta text-muted">{t('routes.tokens.fieldToken')}</p>
          <p className="min-w-0 break-all font-mono text-secondary">
            {tokenRow?.display}
          </p>
          <div className="flex flex-wrap items-center gap-1.5">
            {canCopyToken ? (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setRevealed((current) => !current)}
                >
                  {revealed ? t('common.hideSecret') : t('common.showSecret')}
                </Button>
                <Button variant="outline" size="sm" onClick={copyToken}>
                  {t('routes.tokens.copy')}
                </Button>
              </>
            ) : null}
            {onEditKey ? (
              <Button variant="outline" size="sm" onClick={onEditKey}>
                {t('routes.tokens.editKey')}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </SideInspectPanel>
  );
}
