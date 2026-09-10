import { X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import { composerQueuedFollowUpView } from './chat-composer-model';
import type { QueuedFollowUpItem } from './chat-grok-follow-up';

export function ChatQueuedFollowUpList({
  items,
  onCancelItem,
  onCancelAll,
}: {
  items: readonly QueuedFollowUpItem[];
  onCancelItem?: (id: string) => void;
  onCancelAll?: () => void;
}) {
  const { t } = useI18n();
  const view = composerQueuedFollowUpView(items);
  if (!view) return null;
  return (
    <div
      className="mx-3 mb-1 space-y-1 rounded-btn bg-subtle px-2 py-1"
      data-help="chat-queued-follow-ups"
    >
      <div className="flex items-center gap-2">
        <p className="min-w-0 flex-1 text-meta text-secondary" role="status" aria-live="polite">
          {t('chat.composer.queuedCount', { count: view.count })}
          {' · '}
          {t('chat.composer.queuedHint')}
        </p>
        {onCancelAll ? (
          <Button type="button" size="sm" variant="ghost" onClick={onCancelAll}>
            {t('chat.composer.cancelAllQueued')}
          </Button>
        ) : null}
      </div>
      <ul className="space-y-1">
        {view.items.map((item) => (
          <li key={item.id} className="flex items-center gap-2">
            <p className="min-w-0 flex-1 truncate text-meta text-primary">{item.text}</p>
            {onCancelItem ? (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-auto shrink-0 px-1 py-0.5"
                onClick={() => onCancelItem(item.id)}
              >
                <X className="size-3.5" />
                {t('chat.composer.cancelQueuedItem')}
              </Button>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}
