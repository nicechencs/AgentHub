import { useI18n } from '@/components/shared/LanguageProvider';
import { RouteTraceFlowDiagram } from '@/components/shared/RouteTraceFlowDiagram';
import { cn } from '@/lib/utils';

const LEGEND_TRACE = {
  requestId: 'legend',
  at: '',
  method: 'POST',
  path: '/v1/messages',
  httpStatus: 200,
  ok: true,
  localAuth: { status: 'skipped' as const },
  pool: { status: 'skipped' as const },
  conversion: { status: 'skipped' as const, path: '' },
  upstreamAuth: { status: 'skipped' as const },
  upstream: { status: 'skipped' as const },
};

/**
 * Reference flow diagram for route monitoring pages.
 */
export function RouteTracePipelineLegend({
  className,
  poolLabels,
  upstreamUrls,
}: {
  className?: string;
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
        row={LEGEND_TRACE}
        compact
        previewPoolLabels={poolLabels}
        previewUpstreamUrls={upstreamUrls}
      />
    </div>
  );
}
