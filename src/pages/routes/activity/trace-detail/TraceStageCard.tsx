import { ChevronDown, Minus } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import {
  ActivityTraceStageIcon,
  activityTraceStageTone,
} from '../ActivityTraceStageDisplay';
import { activityTraceStageStatusLabel } from '../activity-trace-list-model';
import { TraceStageDetails } from './TraceStageDetails';
import type { DetailStageStatus, TraceStageViewModel } from './trace-stage-types';

function statusCardTone(status: DetailStageStatus): string {
  if (status === 'ok') return 'border-success/40 bg-success/5';
  if (status === 'failed') return 'border-danger/50 bg-danger/5';
  if (status === 'skipped') return 'border-border/60 bg-subtle/40 opacity-70';
  if (status === 'unrecorded') return 'border-border/60 bg-subtle/30';
  return 'border-accent/40 bg-accent/5';
}

export function TraceStageCard({
  stage,
  index,
  expanded,
  onToggle,
}: {
  stage: TraceStageViewModel;
  index: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const statusLabel = stage.status === 'unrecorded'
    ? t('routes.trace.detail.unrecorded')
    : activityTraceStageStatusLabel(stage.status, t);
  return (
    <li className="relative pl-5" data-detail-stage={stage.id} data-stage-status={stage.status}>
      {index > 0 ? <span className="absolute bottom-1/2 left-[0.4375rem] top-[-0.75rem] w-px bg-border" aria-hidden /> : null}
      <span className="absolute left-0 top-3.5 z-10 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-panel" aria-hidden>
        {stage.status === 'unrecorded'
          ? <Minus className="h-3.5 w-3.5 text-muted" />
          : <ActivityTraceStageIcon status={stage.status} />}
      </span>
      <button
        type="button"
        className={cn(
          'w-full rounded-card border px-3 py-2.5 text-left transition-colors hover:border-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          statusCardTone(stage.status),
        )}
        aria-expanded={expanded}
        aria-controls={`trace-stage-${stage.id}`}
        onClick={onToggle}
      >
        <span className="flex items-center gap-2">
          <span className="min-w-0 flex-1">
            <span className="block text-sm font-medium text-primary">{stage.title}</span>
            {stage.summary ? <span className="mt-0.5 block truncate text-meta text-muted">{stage.summary}</span> : null}
          </span>
          <span className={cn('shrink-0 text-meta', stage.status === 'unrecorded' ? 'text-muted' : activityTraceStageTone(stage.status))}>
            {statusLabel}
          </span>
          <ChevronDown className={cn('h-4 w-4 shrink-0 text-muted transition-transform', expanded && 'rotate-180')} aria-hidden />
        </span>
      </button>
      {expanded ? (
        <div id={`trace-stage-${stage.id}`} className="mx-1 rounded-b-card border border-t-0 border-border bg-panel px-3 py-2.5">
          <TraceStageDetails details={stage.details} />
        </div>
      ) : null}
    </li>
  );
}
