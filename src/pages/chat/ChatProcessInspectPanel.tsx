import { useEffect, useId } from 'react';
import { PanelRightClose } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import type { AgentProcessView } from '@/lib/chat-process';
import { formatProcessHeadline, phaseFromMessageStatus } from '@/lib/chat-process';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { cn } from '@/lib/utils';
import { ChatProcessPanel } from './ChatProcessPanel';

export function ChatProcessInspectPanel({
  view,
  messageStatus,
  exitCode,
  open,
  onClose,
  className,
}: {
  view: AgentProcessView | undefined;
  messageStatus?: string;
  exitCode?: number | null;
  open: boolean;
  onClose: () => void;
  className?: string;
}) {
  const { t } = useI18n();
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  const effectivePhase = view
    ? messageStatus && messageStatus !== 'running'
      ? phaseFromMessageStatus(messageStatus)
      : view.phase
    : null;
  const title = view && effectivePhase
    ? formatProcessHeadline(view.steps, effectivePhase, t)
    : t('chat.process.runDetails');

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        className,
      )}
      aria-labelledby={titleId}
      data-help="chat-process-inspect"
    >
      <header className="shrink-0 border-b border-border">
        <div className="flex h-10 items-center gap-1.5 overflow-x-auto px-3">
          <h2
            id={titleId}
            className="min-w-0 flex-1 truncate text-sm font-semibold leading-tight text-primary"
          >
            {title}
          </h2>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={t('chat.preview.collapse')}
            title={t('chat.preview.collapse')}
            onClick={onClose}
          >
            <PanelRightClose className="h-4 w-4" />
          </Button>
        </div>
      </header>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden px-4 py-3">
        {view ? (
          <ChatProcessPanel view={view} messageStatus={messageStatus} exitCode={exitCode} />
        ) : (
          <p className="text-meta text-muted">{t('chat.process.waitingLogs')}</p>
        )}
      </div>
    </aside>
  );
}
