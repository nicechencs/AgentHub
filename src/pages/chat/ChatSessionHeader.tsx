import { useEffect, useRef, useState } from 'react';
import { Copy, FolderOpen, PanelLeftOpen, Settings2, ShieldAlert, Terminal } from 'lucide-react';
import { ChromeActions } from '@/components/layout/ChromeActions';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { copyTextToClipboard } from '@/components/shared/CopyTextButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import { Hint } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  autoApproveActive,
  autoApproveEffect,
  autoApproveHint,
  conversationResumeCommand,
  conversationTitle,
  cwdShortName,
} from './chat-model';

export function ChatSessionHeader({
  active,
  railOpen,
  recordText,
  onExpandRail,
  onRename,
  onOpenSettings,
  onPickWorkingDirectory,
}: {
  active: Conversation | null;
  railOpen: boolean;
  recordText?: string;
  onExpandRail: () => void;
  onRename: (next: string) => Promise<boolean>;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [editing, setEditing] = useState(false);
  const [draftTitle, setDraftTitle] = useState(active?.title ?? '');
  const cancelledRef = useRef(false);
  const committedRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setEditing(false);
    setDraftTitle(active?.title ?? '');
  }, [active?.id]);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  const selectedAgent = active?.agentIds[0] ?? null;
  const approveOn = autoApproveActive(Boolean(active?.allowDangerous), selectedAgent);

  async function commit() {
    if (cancelledRef.current) {
      cancelledRef.current = false;
      return;
    }
    if (committedRef.current) return;
    committedRef.current = true;
    setEditing(false);
    const ok = await onRename(draftTitle);
    if (!ok) setDraftTitle(active?.title ?? '');
  }

  return (
    <header
      className={cn(
        'flex h-10 shrink-0 items-center gap-2 border-b border-border',
        pageRhythm.chatChromeX,
      )}
      data-help="chat-header"
    >
      {!railOpen && (
        <Hint label={t('chat.rail.expandHistory')}>
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
            onClick={onExpandRail}
            aria-label={t('chat.rail.expandHistory')}
          >
            <PanelLeftOpen className="h-4 w-4" />
          </button>
        </Hint>
      )}
      <div className="min-w-0 flex-1">
        {active && editing ? (
          <Input
            ref={inputRef}
            value={draftTitle}
            aria-label={t('chat.header.titleAria')}
            className="h-7 max-w-xs font-semibold"
            onChange={(e) => setDraftTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void commit();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                cancelledRef.current = true;
                setDraftTitle(active.title);
                setEditing(false);
              }
            }}
            onBlur={() => void commit()}
          />
        ) : (
          <button
            type="button"
            className="max-w-full truncate text-left text-body font-semibold text-primary"
            onClick={() => {
              if (!active) return;
              committedRef.current = false;
              cancelledRef.current = false;
              setDraftTitle(active.title);
              setEditing(true);
            }}
            disabled={!active}
          >
            {active ? conversationTitle(t, active.title) : t('chat.header.conversation')}
          </button>
        )}
      </div>
      {active && (
        <div className="flex min-w-0 shrink-0 items-center gap-1.5">
          <Hint label={recordText ? t('common.copyRecord') : t('common.copyRecordEmpty')}>
            <button
              type="button"
              className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary disabled:opacity-40"
              disabled={!recordText}
              aria-label={t('common.copyRecord')}
              onClick={() => {
                if (!recordText) {
                  toast({ title: t('common.copyRecordEmpty'), variant: 'danger' });
                  return;
                }
                void copyTextToClipboard(recordText).then(
                  () => toast({ title: t('common.copied'), variant: 'success' }),
                  () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
                );
              }}
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
          </Hint>
          <Hint label={active.cwd || t('chat.header.pickCwd')}>
            <button
              type="button"
              onClick={onPickWorkingDirectory}
              data-help="chat-cwd"
              className={cn(
                'inline-flex h-7 max-w-[9rem] items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta',
                active.cwd ? 'text-secondary hover:bg-hover' : 'text-warning hover:bg-hover',
              )}
            >
              <FolderOpen className="h-3 w-3 shrink-0" />
              <span className="truncate">
                {active.cwd ? cwdShortName(active.cwd, t) : t('chat.header.cwdUnset')}
              </span>
            </button>
          </Hint>
          {active.nativeSessionId && (
            <Hint
              label={t('chat.header.nativeSession', {
                id: shortenId(active.nativeSessionId, 16),
              })}
            >
              <button
                type="button"
                className="inline-flex h-7 max-w-[11rem] items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover"
                onClick={() => {
                  const command = conversationResumeCommand(active);
                  if (!command) {
                    toast({ title: t('chat.header.noResumeCommand'), variant: 'danger' });
                    return;
                  }
                  void navigator.clipboard.writeText(command).then(
                    () =>
                      toast({
                        title: t('chat.header.resumeCommandCopied'),
                        description: t('chat.header.resumeCommandCopiedHint'),
                      }),
                    () => toast({ title: t('chat.bubble.copyFailed'), variant: 'danger' }),
                  );
                }}
                aria-label={t('chat.header.copyResumeCommand')}
              >
                <Terminal className="h-3 w-3 shrink-0" />
                <span className="truncate">{shortenId(active.nativeSessionId, 10)}</span>
              </button>
            </Hint>
          )}
          {approveOn && (
            <Hint label={autoApproveHint(t, autoApproveEffect(selectedAgent))}>
              <button
                type="button"
                onClick={onOpenSettings}
                className="inline-flex h-7 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-warning hover:bg-hover"
              >
                <ShieldAlert className="h-3 w-3 shrink-0" />
                {t('chat.header.autoApprove')}
              </button>
            </Hint>
          )}
          <Hint label={t('chat.header.sessionSettings')}>
            <button
              type="button"
              className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
              aria-label={t('chat.header.sessionSettings')}
              onClick={onOpenSettings}
            >
              <Settings2 className="h-4 w-4" />
            </button>
          </Hint>
        </div>
      )}
      <ChromeActions />
    </header>
  );
}

function shortenId(id: string, max: number): string {
  return id.length <= max ? id : `${id.slice(0, max - 1)}…`;
}
