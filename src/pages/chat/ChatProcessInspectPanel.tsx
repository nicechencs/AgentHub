import { useI18n } from '@/components/shared/LanguageProvider';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import type { AgentProcessView } from '@/lib/chat-process';
import { formatProcessHeadline, phaseFromMessageStatus } from '@/lib/chat-process';
import { ChatProcessPanel } from './ChatProcessPanel';

export function ChatProcessInspectPanel({
  view,
  messageStatus,
  exitCode,
  open,
  onClose,
  width,
  className,
}: {
  view: AgentProcessView | undefined;
  messageStatus?: string;
  exitCode?: number | null;
  open: boolean;
  onClose: () => void;
  width?: number;
  className?: string;
}) {
  const { t } = useI18n();

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
    <SideInspectPanel
      title={title}
      onClose={onClose}
      width={width}
      className={className}
    >
      <div data-help="chat-process-inspect" className="flex min-h-0 min-w-0 flex-1 flex-col">
        {view ? (
          <ChatProcessPanel view={view} messageStatus={messageStatus} exitCode={exitCode} />
        ) : (
          <p className="text-meta text-muted">{t('chat.process.waitingLogs')}</p>
        )}
      </div>
    </SideInspectPanel>
  );
}
