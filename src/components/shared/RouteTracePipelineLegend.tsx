import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteTraceFlowDiagram, type RouteTraceFlowRow } from '@/components/shared/RouteTraceFlowDiagram';
import { cn } from '@/lib/utils';

const LEGEND_TRACE: RouteTraceFlowRow = {
  requestId: 'legend',
  at: '',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 200,
  ok: true,
  localAuth: { status: 'skipped' },
  pool: { status: 'skipped' },
  conversion: { status: 'skipped', path: '' },
  upstreamAuth: { status: 'skipped' },
  upstream: { status: 'skipped' },
};

/**
 * Reference flow diagram for route monitoring pages.
 */
export function RouteTracePipelineLegend({
  className,
  row,
  poolLabels,
  upstreamUrls,
}: {
  className?: string;
  row?: RouteTraceFlowRow;
  poolLabels?: readonly string[];
  upstreamUrls?: readonly string[];
}) {
  const { t } = useI18n();
  return (
    <div
      className={cn(
        'rounded-card border border-border bg-panel p-3',
        className,
      )}
      data-route-trace-legend
    >
      <p className="mb-2 text-meta font-medium text-primary">{t('routes.trace.legendTitle')}</p>
      <RouteTraceFlowDiagram
        row={row ?? LEGEND_TRACE}
        compact
        previewPoolLabels={poolLabels}
        previewUpstreamUrls={upstreamUrls}
      />
    </div>
  );
}
