import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { RuntimeHostTerminal } from '@/lib/api/chat';
import { cn } from '@/lib/utils';

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
          <p className="font-medium text-primary">{t('chat.runtime.hostCommand')}</p>
          <p className="mt-1 font-mono text-meta text-secondary">{terminal.command}</p>
          {terminal.output.trim() ? (
            <pre className="mt-2 max-h-36 overflow-auto whitespace-pre-wrap break-all rounded-card border border-border/60 bg-canvas px-2 py-1.5 font-mono text-meta leading-relaxed text-primary">
              {terminal.output}
            </pre>
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
