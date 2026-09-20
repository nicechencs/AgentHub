import { useEffect, useRef, useState } from 'react';
import {
  ChevronDown,
  ChevronUp,
  ChevronsUpDown,
  Copy,
  FolderOpen,
  PanelLeftOpen,
  Settings2,
  ShieldAlert,
  Terminal,
} from 'lucide-react';
import { ChromeActions } from '@/components/layout/ChromeActions';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { copyTextToClipboard } from '@/components/shared/CopyTextButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint } from '@/components/ui/tooltip';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import type { Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import { sessionSwitchNeighbors } from './chat-session-switch';
import { isKiroChatAgent } from './chat-kiro-model';
import { sessionAllowAlwaysActive } from './chat-runtime-model';
import type { RuntimeSnapshot } from '@/lib/api/chat';
import {
  autoApproveActive,
  autoApproveEffect,
  autoApproveHint,
  canRebindConversationCwd,
  conversationCwdMissing,
  conversationResumeCommand,
  conversationTitle,
  cwdShortName,
} from './chat-model';

export function ChatSessionHeader({
  active,
  railOpen,
  recordText,
  sessions,
  sendingConversationIds = [],
  onExpandRail,
  onRename,
  onFocus,
  onOpenSettings,
  onPickWorkingDirectory,
  runtimeLocked = false,
  runtime = null,
}: {
  active: Conversation | null;
  railOpen: boolean;
  recordText?: string;
  sessions: readonly Conversation[];
  sendingConversationIds?: readonly string[];
  onExpandRail: () => void;
  onRename: (next: string) => Promise<boolean>;
  onFocus: (id: string) => void;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
  runtimeLocked?: boolean;
  runtime?: Pick<RuntimeSnapshot, 'sessionAllowAlways'> | null;
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
  const kiroPermissions = isKiroChatAgent(selectedAgent);
  const sessionAlways = sessionAllowAlwaysActive(runtime);

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
        'flex h-11 shrink-0 items-center gap-2 border-b border-border',
        pageRhythm.chatChromeX,
      )}
      data-help="chat-header"
    >
      {!railOpen && (
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="text-muted"
          onClick={onExpandRail}
          title={t('chat.rail.expandHistory')}
          aria-label={t('chat.rail.expandHistory')}
        >
          <PanelLeftOpen className="h-4 w-4" />
        </Button>
      )}
      <div className="min-w-0 flex-1">
        {!railOpen && sessions.length > 0 ? (
          <ChatSessionSwitcher
            sessions={sessions}
            active={active}
            sendingConversationIds={sendingConversationIds}
            onFocus={onFocus}
          />
        ) : active && editing ? (
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
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="text-muted"
            disabled={!recordText}
            title={recordText ? t('common.copyRecord') : t('common.copyRecordEmpty')}
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
          </Button>
          <Hint
            label={
              conversationCwdMissing(active)
                ? t('chat.cwd.missing')
                : runtimeLocked
                  ? t('chat.runtimeOps.sessionLocked')
                  : active.cwd || t('chat.header.pickCwd')
            }
          >
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onPickWorkingDirectory}
              disabled={!canRebindConversationCwd(active, runtimeLocked)}
              data-help="chat-cwd"
              className={cn(
                'max-w-[7rem]',
                (!active.cwd || conversationCwdMissing(active)) && 'text-warning',
              )}
            >
              <FolderOpen className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">
                {conversationCwdMissing(active)
                  ? t('chat.header.cwdMissing')
                  : active.cwd
                    ? cwdShortName(active.cwd, t)
                    : t('chat.header.cwdUnset')}
              </span>
            </Button>
          </Hint>
          {active.nativeSessionId && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="max-w-[11rem]"
              title={t('chat.header.nativeSession', {
                id: shortenId(active.nativeSessionId, 16),
              })}
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
              <Terminal className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{shortenId(active.nativeSessionId, 10)}</span>
            </Button>
          )}
          {sessionAlways ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onOpenSettings}
              title={
                kiroPermissions
                  ? t('chat.runtime.sessionRememberedOnHintKiro')
                  : t('chat.runtime.sessionRememberedOnHint')
              }
              data-help="chat-header-always-allow"
              className="text-muted"
            >
              {t('chat.runtime.sessionRemembered')}
            </Button>
          ) : null}
          {kiroPermissions ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onOpenSettings}
              title={approveOn ? t('chat.kiro.permissionFullHint') : t('chat.kiro.permissionAskHint')}
              className={approveOn ? 'text-warning' : 'text-muted'}
            >
              <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
              {approveOn ? t('chat.kiro.permissionFull') : t('chat.kiro.permissionAsk')}
            </Button>
          ) : approveOn ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onOpenSettings}
              title={autoApproveHint(t, autoApproveEffect(selectedAgent), selectedAgent)}
              className="text-warning"
            >
              <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
              {t('chat.header.autoApprove')}
            </Button>
          ) : null}
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="text-muted"
            data-help="chat-settings"
            title={t('chat.header.sessionSettings')}
            aria-label={t('chat.header.sessionSettings')}
            onClick={onOpenSettings}
          >
            <Settings2 className="h-4 w-4" />
          </Button>
        </div>
      )}
      <ChromeActions />
    </header>
  );
}

function shortenId(id: string, max: number): string {
  return id.length <= max ? id : `${id.slice(0, max - 1)}…`;
}

function SendingDot({ sending }: { sending: boolean }) {
  if (!sending) return null;
  return (
    <span aria-hidden className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-accent" data-sending="" />
  );
}

function ChatSessionSwitcher({
  sessions,
  active,
  sendingConversationIds,
  onFocus,
}: {
  sessions: readonly Conversation[];
  active: Conversation | null;
  sendingConversationIds: readonly string[];
  onFocus: (id: string) => void;
}) {
  const { t } = useI18n();
  const neighbors = sessionSwitchNeighbors(sessions, active?.id ?? null);
  const title = active ? conversationTitle(t, active.title) : t('chat.header.conversation');
  const cwdLabel = active ? cwdShortName(active.cwd, t) : t('chat.cwd.unset');
  const sendingHere = Boolean(active && sendingConversationIds.includes(active.id));

  return (
    <div className="flex min-w-0 items-center gap-0.5" data-help="chat-session-switch">
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className="text-muted"
        disabled={!neighbors.prevId}
        title={t('chat.shortcuts.prevSession')}
        aria-label={t('chat.shortcuts.prevSession')}
        aria-keyshortcuts="Alt+ArrowUp"
        onClick={() => {
          if (neighbors.prevId) onFocus(neighbors.prevId);
        }}
      >
        <ChevronUp className="h-4 w-4" />
      </Button>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="min-w-0 max-w-xs justify-start px-1.5"
            aria-label={t('chat.header.switchSession')}
          >
            <SendingDot sending={sendingHere} />
            <span className="min-w-0 truncate font-semibold text-primary">{title}</span>
            <span className="min-w-0 truncate text-meta font-normal text-muted">{cwdLabel}</span>
            <ChevronsUpDown className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-h-72 min-w-[16rem] overflow-y-auto">
          {sessions.map((session) => {
            const selected = active?.id === session.id;
            const sending = sendingConversationIds.includes(session.id);
            return (
              <DropdownMenuItem
                key={session.id}
                className={cn('items-start', selected && 'font-medium')}
                onSelect={() => onFocus(session.id)}
              >
                <SendingDot sending={sending} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{conversationTitle(t, session.title)}</span>
                  <span className="block truncate text-meta text-muted">
                    {cwdShortName(session.cwd, t)}
                  </span>
                </span>
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuContent>
      </DropdownMenu>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className="text-muted"
        disabled={!neighbors.nextId}
        title={t('chat.shortcuts.nextSession')}
        aria-label={t('chat.shortcuts.nextSession')}
        aria-keyshortcuts="Alt+ArrowDown"
        onClick={() => {
          if (neighbors.nextId) onFocus(neighbors.nextId);
        }}
      >
        <ChevronDown className="h-4 w-4" />
      </Button>
    </div>
  );
}
