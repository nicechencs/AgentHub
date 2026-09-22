import { useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import type { RuntimePlanEntry } from '@/lib/api/chat';
import { ChatExpandAffordance } from './ChatExpandAffordance';
import {
  runtimePlanEntryTone,
  runtimePlanProgress,
  runtimePlanStatusKey,
  visibleRuntimePlan,
} from './chat-runtime-model';

export function ChatPlanBar({
  plan,
  defaultOpen = true,
}: {
  plan?: RuntimePlanEntry[] | null;
  defaultOpen?: boolean;
}) {
  const { t } = useI18n();
  const entries = visibleRuntimePlan(plan);
  const [open, setOpen] = useState(defaultOpen);
  if (entries.length === 0) return null;
  const progress = runtimePlanProgress(entries);
  const listId = 'chat-plan-bar-list';
  const summary = [
    t('chat.runtime.planProgress', { done: progress.done, total: progress.total }),
    progress.live > 0 ? `${t('chat.runtime.planStatusLive')} ${progress.live}` : null,
    progress.failed > 0 ? `${t('chat.runtime.planStatusFailed')} ${progress.failed}` : null,
  ].filter(Boolean).join(' · ');
  return (
    <section
      className="mb-2 rounded-card border border-border bg-subtle px-3 py-2 text-meta"
      data-help="chat-plan-bar"
      aria-label={t('chat.runtime.plan')}
    >
      <button
        type="button"
        className="group flex w-full min-w-0 items-center gap-2 text-left"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="min-w-0 flex-1 truncate">
          <span className="font-medium text-secondary">{t('chat.runtime.plan')}</span>
          <span className="text-muted">{` · ${summary}`}</span>
        </span>
        <ChatExpandAffordance
          expanded={open}
          label={open ? t('chat.runtime.planCollapse') : t('chat.runtime.planExpand')}
        />
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
      ) : null}
    </section>
  );
}
