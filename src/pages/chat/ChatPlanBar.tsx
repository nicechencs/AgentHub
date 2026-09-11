import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import type { RuntimePlanEntry } from '@/lib/api/chat';
import { runtimePlanEntryTone, visibleRuntimePlan } from './chat-runtime-model';

export function ChatPlanBar({ plan }: { plan?: RuntimePlanEntry[] | null }) {
  const { t } = useI18n();
  const entries = visibleRuntimePlan(plan);
  if (entries.length === 0) return null;
  return (
    <section
      className="mb-2 rounded-card border border-border bg-subtle px-3 py-2 text-meta"
      data-help="chat-plan-bar"
      aria-label={t('chat.runtime.plan')}
    >
      <p className="font-medium text-secondary">{t('chat.runtime.plan')}</p>
      <ol className="mt-1 space-y-0.5">
        {entries.map((entry, index) => {
          const tone = runtimePlanEntryTone(entry.status);
          return (
            <li
              key={`${index}:${entry.content}`}
              className={cn(
                'min-w-0 truncate text-secondary',
                tone === 'live' && 'font-medium text-primary',
                tone === 'done' && 'text-muted',
              )}
            >
              {entry.content}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
