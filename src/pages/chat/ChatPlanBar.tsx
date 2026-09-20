import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import type { RuntimePlanEntry } from '@/lib/api/chat';
import {
  runtimePlanEntryTone,
  runtimePlanProgress,
  runtimePlanStatusKey,
  visibleRuntimePlan,
} from './chat-runtime-model';

export function ChatPlanBar({ plan }: { plan?: RuntimePlanEntry[] | null }) {
  const { t } = useI18n();
  const entries = visibleRuntimePlan(plan);
  const [open, setOpen] = useState(true);
  if (entries.length === 0) return null;
  const progress = runtimePlanProgress(entries);
  const live = entries.find((entry) => runtimePlanEntryTone(entry.status) === 'live');
  const listId = 'chat-plan-bar-list';
  return (
    <section
      className="mb-2 rounded-card border border-border bg-subtle px-3 py-2 text-meta"
      data-help="chat-plan-bar"
      aria-label={t('chat.runtime.plan')}
    >
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-2 text-left"
        aria-expanded={open}
        aria-controls={open || live ? listId : undefined}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="min-w-0 flex-1">
          <span className="font-medium text-secondary">{t('chat.runtime.plan')}</span>
          <span className="mt-0.5 block text-muted">
            {t('chat.runtime.planProgress', { done: progress.done, total: progress.total })}
            {progress.live > 0
              ? ` · ${t('chat.runtime.planStatusLive')} ${progress.live}`
              : null}
            {progress.failed > 0
              ? ` · ${t('chat.runtime.planStatusFailed')} ${progress.failed}`
              : null}
          </span>
        </span>
        <ChevronDown
          className={cn('h-4 w-4 shrink-0 text-muted transition-transform', open && 'rotate-180')}
          aria-hidden
        />
        <span className="sr-only">
          {open ? t('chat.runtime.planCollapse') : t('chat.runtime.planExpand')}
        </span>
      </button>
      {open ? (
        <ol id={listId} className="mt-2 space-y-1">
          {entries.map((entry, index) => {
            const tone = runtimePlanEntryTone(entry.status);
            return (
              <li
                key={`${index}:${entry.content}`}
                className={cn(
                  'grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-2',
                  tone === 'live' && 'font-medium text-primary',
                  tone === 'done' && 'text-muted',
                  tone === 'failed' && 'text-danger',
                  tone === 'pending' && 'text-secondary',
                )}
              >
                <span className="shrink-0 text-muted">
                  {t(runtimePlanStatusKey(entry.status))}
                </span>
                <span className="min-w-0 flex-1 whitespace-normal break-words">{entry.content}</span>
              </li>
            );
          })}
        </ol>
      ) : live ? (
        <p id={listId} className="mt-2 min-w-0 font-medium text-primary">
          <span className="text-muted">{t(runtimePlanStatusKey(live.status))} · </span>
          {live.content}
        </p>
      ) : null}
    </section>
  );
}
