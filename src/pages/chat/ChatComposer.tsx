import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  type Ref,
} from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Check,
  ChevronDown,
  SendHorizontal,
  Square,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint, Tip } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
import type { AgentId, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  autoApproveFooter,
  blockerCopy,
  blockerPrimaryTarget,
  chatAgentPickerEmptyCopy,
  chatAgentPickerEmptyKind,
  chatShowsUnimportedCurrent,
  COMPOSER_TEXTAREA_MAX_PX,
  COMPOSER_TEXTAREA_MIN_PX,
  composerTextareaMeasuredStyle,
  composerUsesCssFieldSizing,
  type ChatAgentPickerRow,
  type ChatConnectionOption,
  type ChatConnectionPickerView,
  type ChatSendBlocker,
} from './chat-model';

export function ChatComposer({
  draft,
  setDraft,
  sending,
  active,
  connectionOptions,
  primaryAgent,
  agentPickerLabel,
  connectionView,
  switchingProvider,
  hiddenIds,
  pickerRows,
  agentsReady,
  blockers,
  connectionCaption,
  walletError,
  onRetryWallet,
  onRetryStatus,
  onSend,
  onCancel,
  onSelectAgent,
  onSwitchConnection,
  modelOptions,
  currentModel,
  switchingModel,
  onSwitchModel,
  onOpenSettings,
  onPickWorkingDirectory,
  onFocusConversation,
  fillHeight = false,
  paneHeight = null,
  paneRef,
}: {
  draft: string;
  setDraft: (v: string) => void;
  sending: boolean;
  active: Conversation;
  connectionOptions: ChatConnectionOption[];
  primaryAgent: AgentId | null;
  agentPickerLabel: string;
  connectionView: ChatConnectionPickerView;
  switchingProvider: boolean;
  hiddenIds: Set<AgentId>;
  pickerRows: ChatAgentPickerRow[];
  agentsReady: boolean;
  blockers: ChatSendBlocker[];
  connectionCaption: string | null;
  walletError?: unknown;
  onRetryWallet?: () => void;
  onRetryStatus?: () => void;
  onSend: () => void;
  onCancel: () => void;
  onSelectAgent: (id: AgentId) => void;
  onSwitchConnection: (ticketId: string) => void;
  modelOptions: string[];
  currentModel: string | null;
  switchingModel: boolean;
  onSwitchModel: (model: string) => void;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
  onFocusConversation: (id: string) => void;
  fillHeight?: boolean;
  paneHeight?: number | null;
  paneRef?: Ref<HTMLDivElement>;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const firstBlocker = blockers[0] ?? null;
  const hiddenBlocked = firstBlocker?.kind === 'hiddenAgents' ||
    active.agentIds.some((id) => hiddenIds.has(id));
  const sendingElsewhere = blockers.some((b) => b.kind === 'sendingElsewhere');
  const canSend = Boolean(draft.trim()) && blockers.length === 0 && !sending;
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const syncTextareaHeight = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    if (fillHeight) {
      el.style.height = '';
      el.style.overflowY = '';
      return;
    }
    if (composerUsesCssFieldSizing()) return;
    el.style.overflowY = 'hidden';
    el.style.height = '0px';
    const layout = composerTextareaMeasuredStyle(el.scrollHeight);
    el.style.height = layout.height;
    el.style.overflowY = layout.overflowY;
  }, [fillHeight]);

  useLayoutEffect(() => {
    syncTextareaHeight();
  }, [draft, fillHeight, syncTextareaHeight]);

  useEffect(() => {
    const onResize = () => syncTextareaHeight();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [syncTextareaHeight]);

  const textareaDisabled = sending || hiddenBlocked || sendingElsewhere;
  const sendHint = firstBlocker ? blockerCopy(t, firstBlocker).text : t('chat.composer.send');
  const selectedAgent = active.agentIds[0] ?? '';
  const approveFooter = autoApproveFooter(t, active.allowDangerous, active.agentIds[0] ?? null);
  const pickerEmpty = chatAgentPickerEmptyKind({
    agentsReady,
    rowCount: pickerRows.length,
  });
  const pickerEmptyCopy = pickerEmpty ? chatAgentPickerEmptyCopy(t, pickerEmpty) : null;

  return (
    <>
      {walletError && !firstBlocker ? (
        <Notice
          tone="warning"
          className="mb-2"
          actionLabel={onRetryWallet ? t('chrome.error.retry') : undefined}
          onAction={onRetryWallet}
        >
          {t('connections.page.walletError')}
        </Notice>
      ) : null}
      {firstBlocker && (
        <BlockerNotice
          blocker={firstBlocker}
          onGoAgents={() => navigate('/agents')}
          onGoConnections={() =>
            navigate(primaryAgent ? `/connections?agent=${primaryAgent}` : '/connections')
          }
          onOpenSettings={onOpenSettings}
          onPickWorkingDirectory={onPickWorkingDirectory}
          onFocusConversation={onFocusConversation}
          onCancel={onCancel}
          onRetryStatus={onRetryStatus}
        />
      )}
      <div
        ref={paneRef}
        className={cn(
          'flex min-h-0 flex-col',
          fillHeight ? 'overflow-hidden' : 'shrink-0',
        )}
        style={paneHeight != null ? { height: paneHeight } : undefined}
      >
        <div
          className={cn(
            'rounded-composer border border-border bg-panel shadow-xs',
            fillHeight && 'flex min-h-0 flex-1 flex-col overflow-hidden',
          )}
        >
        <textarea
          ref={textareaRef}
          className={cn(
            'block w-full resize-none overflow-x-hidden overflow-y-auto break-words bg-transparent',
            fillHeight ? 'min-h-0 flex-1' : '[field-sizing:content]',
            'px-4 pb-2 pt-3 text-body leading-[1.45] outline-none placeholder:text-muted',
            'disabled:cursor-not-allowed disabled:opacity-60',
          )}
          style={
            fillHeight
              ? undefined
              : { minHeight: COMPOSER_TEXTAREA_MIN_PX, maxHeight: COMPOSER_TEXTAREA_MAX_PX }
          }
          placeholder={t('chat.composer.placeholder')}
          rows={1}
          value={draft}
          disabled={textareaDisabled}
          onChange={(e) => setDraft(e.target.value)}
          onInput={syncTextareaHeight}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              if (canSend) onSend();
            }
          }}
          aria-label={t('chat.composer.inputAria')}
        />
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/50 px-2 py-2">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                disabled={sending || sendingElsewhere}
                className="inline-flex h-7 max-w-36 items-center gap-1.5 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover disabled:opacity-50"
              >
                {active.agentIds[0] && <AgentLogo agentId={active.agentIds[0]} size="sm" />}
                <span className="truncate">{agentPickerLabel}</span>
                <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              <DropdownMenuLabel>{t('chat.composer.selectAgent')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={selectedAgent}
                onValueChange={(id) => onSelectAgent(id as AgentId)}
              >
                {pickerRows.map((row) => (
                  <DropdownMenuRadioItem
                    key={row.id}
                    value={row.id}
                    disabled={sending || sendingElsewhere || !row.selectable}
                  >
                    <span
                      className={cn(
                        'flex items-center gap-2',
                        !row.selectable && 'text-muted',
                      )}
                    >
                      <AgentLogo agentId={row.id} size="sm" />
                      {agentDisplayName(row.id)}
                      {row.reason === 'noAuth' && (
                        <span className="text-meta text-muted">{t('chat.composer.noAuth')}</span>
                      )}
                      {row.reason === 'envNotReady' && (
                        <span className="text-meta text-muted">{t('chat.composer.envNotReady')}</span>
                      )}
                    </span>
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
              {pickerEmptyCopy && (
                <div className="px-2 py-2">
                  <p className="text-meta text-muted">{pickerEmptyCopy.text}</p>
                  {pickerEmptyCopy.action && (
                    <Button
                      size="sm"
                      variant="outline"
                      className="mt-2"
                      onClick={() => navigate('/agents')}
                    >
                      {pickerEmptyCopy.action}
                    </Button>
                  )}
                </div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <Hint label={connectionCaption ?? undefined}>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  disabled={
                    !primaryAgent ||
                    sending ||
                    sendingElsewhere ||
                    switchingProvider ||
                    Boolean(primaryAgent && hiddenIds.has(primaryAgent))
                  }
                  className="inline-flex h-7 max-w-44 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover disabled:opacity-50"
                  aria-label={connectionCaption ?? t('chat.composer.switchConnection')}
                >
                  <span className="min-w-0 truncate">
                    {connectionView.label}
                    {connectionView.subtitle ? (
                      <span className="text-muted"> · {connectionView.subtitle}</span>
                    ) : null}
                  </span>
                  <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
                </button>
              </DropdownMenuTrigger>
            </Hint>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuLabel>
                {primaryAgent
                  ? t('chat.composer.switchConnectionNamed', { name: agentDisplayName(primaryAgent) })
                  : t('chat.composer.switchConnection')}
              </DropdownMenuLabel>
              {connectionCaption && (
                <p className="px-2 pb-1.5 text-meta text-muted">{connectionCaption}</p>
              )}
              <DropdownMenuSeparator />
              {chatShowsUnimportedCurrent(
                connectionOptions,
                connectionView.currentLoginTitle,
              ) && (
                <DropdownMenuItem disabled>
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <Check className="h-3.5 w-3.5 shrink-0 text-accent" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate">{connectionView.currentLoginTitle}</span>
                      {connectionView.currentLoginSubtitle ? (
                        <span className="block truncate text-meta text-muted">
                          {connectionView.currentLoginSubtitle}
                        </span>
                      ) : null}
                    </span>
                  </span>
                </DropdownMenuItem>
              )}
              {connectionOptions.map((option) => {
                const isCurrent = option.isCurrent;
                return (
                  <DropdownMenuItem
                    key={option.ticketId}
                    disabled={isCurrent || switchingProvider}
                    onClick={() => onSwitchConnection(option.ticketId)}
                  >
                    <span className="flex min-w-0 flex-1 items-center gap-2">
                      {isCurrent ? (
                        <Check className="h-3.5 w-3.5 shrink-0 text-accent" />
                      ) : (
                        <span className="w-3.5 shrink-0" />
                      )}
                      <span className="min-w-0 flex-1">
                        <span className="block truncate">{option.title}</span>
                        {option.subtitle ? (
                          <span className="block truncate text-meta text-muted">
                            {option.subtitle}
                          </span>
                        ) : null}
                      </span>
                    </span>
                  </DropdownMenuItem>
                );
              })}
              {connectionView.emptyHint && connectionOptions.length === 0 && (
                <p className="px-2 py-1.5 text-meta text-muted">{connectionView.emptyHint}</p>
              )}
              {primaryAgent && (
                <div className="px-2 py-2">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => navigate(`/connections?agent=${primaryAgent}`)}
                  >
                    {connectionView.manageLabel}
                  </Button>
                </div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          {modelOptions.length > 0 ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  disabled={sending || sendingElsewhere || switchingProvider || switchingModel}
                  className="inline-flex h-7 max-w-40 items-center gap-1 rounded-btn border border-border bg-subtle px-2 text-meta text-secondary hover:bg-hover disabled:opacity-50"
                  aria-label={t('chat.composer.switchModel')}
                >
                  <span className="min-w-0 truncate">
                    {currentModel || t('chat.composer.switchModel')}
                  </span>
                  <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-64">
                <DropdownMenuLabel>{t('chat.composer.switchModel')}</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuRadioGroup
                  value={currentModel ?? ''}
                  onValueChange={(id) => onSwitchModel(id)}
                >
                  {modelOptions.map((model) => (
                    <DropdownMenuRadioItem
                      key={model}
                      value={model}
                      disabled={sending || sendingElsewhere || switchingModel}
                    >
                      <span className="truncate">{model}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}

          <Tip
            className={cn(
              'min-w-0 flex-1 truncate text-left text-meta leading-none',
              approveFooter.warning ? 'text-warning/50' : 'text-muted/35',
            )}
            label={approveFooter.text}
          >
            {approveFooter.text}
          </Tip>

          {sending ? (
            <Button size="sm" variant="dangerOutline" onClick={onCancel}>
              <Square className="h-3.5 w-3.5" />
              {t('chat.composer.stop')}
            </Button>
          ) : (
            <Button
              size="icon"
              variant={canSend ? 'default' : 'secondary'}
              className="h-7 w-7 rounded-btn"
              disabled={!canSend}
              onClick={onSend}
              aria-label={t('chat.composer.send')}
              title={sendHint}
            >
              <SendHorizontal className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      </div>
    </>
  );
}

function BlockerNotice({
  blocker,
  onGoAgents,
  onGoConnections,
  onOpenSettings,
  onPickWorkingDirectory,
  onFocusConversation,
  onCancel,
  onRetryStatus,
}: {
  blocker: ChatSendBlocker;
  onGoAgents: () => void;
  onGoConnections: () => void;
  onOpenSettings: () => void;
  onPickWorkingDirectory: () => void;
  onFocusConversation: (id: string) => void;
  onCancel: () => void;
  onRetryStatus?: () => void;
}) {
  const { t } = useI18n();
  const copy = blockerCopy(t, blocker);
  if (blocker.kind === 'sendingElsewhere') {
    return (
      <Notice tone="warning" className="mb-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span>{copy.text}</span>
          <span className="flex items-center gap-1">
            <Button
              size="sm"
              variant="outline"
              onClick={() => onFocusConversation(blocker.conversationId)}
            >
              {copy.primaryAction}
            </Button>
            <Button
              size="sm"
              variant="dangerOutline"
              onClick={onCancel}
            >
              {copy.secondaryAction}
            </Button>
          </span>
        </div>
      </Notice>
    );
  }
  return (
    <Notice
      tone="warning"
      className="mb-2"
      actionLabel={copy.primaryAction}
      onAction={
        {
          agents: onGoAgents,
          connections: onGoConnections,
          'pick-directory': onPickWorkingDirectory,
          settings: onOpenSettings,
          retry: onRetryStatus,
        }[blockerPrimaryTarget(blocker)]
      }
    >
      {copy.text}
    </Notice>
  );
}
