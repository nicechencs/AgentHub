import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';

const STAGE_KEYS = [
  'stageLocalAuth',
  'stagePool',
  'stageConversion',
  'stageUpstreamAuth',
  'stageUpstream',
] as const;

/**
 * Always-visible five-stage legend for route monitoring pages.
 */
export function RouteTracePipelineLegend({ className }: { className?: string }) {
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
      <ol className="flex flex-wrap gap-2">
        {STAGE_KEYS.map((key, index) => (
          <li
            key={key}
            className="inline-flex items-center gap-1.5 rounded-full border border-border bg-subtle px-2.5 py-1 text-meta text-primary"
          >
            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent text-caption font-medium text-accent-foreground">
              {index + 1}
            </span>
            {t(`routes.trace.${key}`)}
          </li>
        ))}
      </ol>
    </div>
  );
}
