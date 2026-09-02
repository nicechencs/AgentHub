import { Check, Minus, X } from 'lucide-react';
import type { RouteTraceStageStatus } from '@/lib/backend/contracts/adapter';

export function activityTraceStageTone(status: RouteTraceStageStatus): string {
  if (status === 'ok') return 'text-success';
  if (status === 'failed') return 'text-danger';
  if (status === 'skipped') return 'text-muted';
  return 'text-secondary';
}

export function ActivityTraceStageIcon({ status }: { status: RouteTraceStageStatus }) {
  if (status === 'ok') return <Check className="h-3.5 w-3.5 text-success" aria-hidden />;
  if (status === 'failed') return <X className="h-3.5 w-3.5 text-danger" aria-hidden />;
  if (status === 'skipped') return <Minus className="h-3.5 w-3.5 text-muted" aria-hidden />;
  return <span className="inline-block h-2 w-2 rounded-full bg-muted" aria-hidden />;
}
