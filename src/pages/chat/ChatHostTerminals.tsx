import { ContentCopyButton } from '@/components/shared/CopyTextButton';
import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RuntimeHostTerminal } from '@/lib/api/chat';
import { cn } from '@/lib/utils';
import { ResizableRegion, SNIPPET_SURFACE } from './ChatResizableRegion';

export function ChatHostTerminals({
  terminals,
  onKill,
}: {
  terminals: RuntimeHostTerminal[];
  onKill: (terminalId: string) => Promise<void>;
}) {
  const { t } = useI18n();
  if (terminals.length === 0) return null;
  return (
    <div className="w-full space-y-2 py-2">
      {terminals.map((terminal) => (
        <section
          key={terminal.id}
          className="rounded-card border border-border bg-subtle p-3 text-body"
          data-help="chat-host-terminal"
        >
          <div className="flex items-center justify-between gap-2">
            <p className="font-medium text-primary">{t('chat.runtime.hostCommand')}</p>
            <ContentCopyButton text={terminal.output || terminal.command} />
          </div>
          <p className="mt-1 font-mono text-meta text-secondary">{terminal.command}</p>
          {terminal.output.trim() ? (
            <ResizableRegion pane="code" label={t('chat.process.resizeCode')} className="mt-2">
              {(height) => (
                <pre style={{ height }} className={cn(SNIPPET_SURFACE, 'px-3 py-2')}>
                  {terminal.output}
                </pre>
              )}
            </ResizableRegion>
          ) : null}
          {terminal.exitCode != null ? (
            <p className="mt-1 text-meta text-muted">
              {t('chat.process.exitCode', { code: terminal.exitCode })}
            </p>
          ) : null}
          {terminal.running ? (
            <div className="mt-3">
              <Button
                size="sm"
                variant="ghost"
                className={cn('text-danger')}
                onClick={() => void onKill(terminal.id)}
              >
                {t('chat.runtime.stopCommand')}
              </Button>
            </div>
          ) : null}
        </section>
      ))}
    </div>
  );
}
