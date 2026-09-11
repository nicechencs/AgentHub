import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
} from 'react';
import { useNavigate } from 'react-router-dom';
import {
  ArrowUp,
  Check,
  ChevronDown,
  MoreHorizontal,
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
import type { AgentKey, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import { composerNativeEditChord } from './chat-model';
import {
  composerEnterShouldSubmit,
  composerFooterControl,
  composerPrimaryAction,
  composerShortcutKind,
  composerShortcutMessageKey,
  composerShouldRestoreFocus,
  composerStopMessageKey,
  composerStopTitle,
  composerSubmitMessageKey,
} from './chat-composer-model';
import {
  composerCapabilityHint,
  composerCompactSecondary,
  composerConnectionTooltip,
  composerHoverHint,
  composerInvitePlaceholder,
  composerShowsHintRow,
} from './chat-empty-state';
import { ChatActionMenu } from './ChatActionMenu';
import type { ChatActionContext, ChatActionDef } from './chat-actions';
import { ChatShortcutsHelp } from './ChatShortcutsHelp';
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
import { ChatQueuedFollowUpList } from './ChatQueuedFollowUpList';
import { chatEffortHint, chatEffortLabel, chatModelDisplayName } from './chat-model-labels';
import type { QueuedFollowUpItem } from './chat-grok-follow-up';

export function ChatComposer({
  draft,
  setDraft,
  sending,
  canceling = false,
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
  onSteer,
  onQueueAfterTurn,
  queuedFollowUps = [],
  onCancelQueuedFollowUp,
  onClearQueuedFollowUp,
  onCancel,
  onSelectAgent,
  onSwitchConnection,
  modelOptions,
  currentModel,
  switchingModel,
  onSwitchModel,
  effortOptions = [],
  currentEffort = null,
  onSwitchEffort,
  onPickWorkingDirectory,
  onDraftKeyDown,
  onPasteImages,
  runtimeControls,
  connectionLocked = false,
  runtimeLocked = false,
  fillHeight = false,
  paneHeight = null,
  paneRef,
  showBlockerBanner = true,
  emptyTranscript = false,
  focusNonce = 0,
  modelMenuOpenNonce = 0,
  commandSearchOpen = false,
  commandIndex = 0,
  actionContext = { hasLatestReply: false, newChatAllowed: true },
  extraActions,
  onRunAction,
  onHoverCommandIndex,
  runtimeEnabled = false,
  steer = false,
}: {
  draft: string;
  setDraft: (v: string) => void;
  sending: boolean;
  canceling?: boolean;
  active: Conversation;
  connectionOptions: ChatConnectionOption[];
  primaryAgent: AgentKey | null;
  agentPickerLabel: string;
  connectionView: ChatConnectionPickerView;
  switchingProvider: boolean;
  hiddenIds: Set<AgentKey>;
  pickerRows: ChatAgentPickerRow[];
  agentsReady: boolean;
  blockers: ChatSendBlocker[];
  connectionCaption: string | null;
  walletError?: unknown;
  onRetryWallet?: () => void;
  onRetryStatus?: () => void;
  onSend: () => void;
  onSteer?: () => void;
  onQueueAfterTurn?: () => void;
  queuedFollowUps?: readonly QueuedFollowUpItem[];
  onCancelQueuedFollowUp?: (id: string) => void;
  onClearQueuedFollowUp?: () => void;
  focusNonce?: number;
  onCancel: () => void;
  onSelectAgent: (id: AgentKey) => void;
  onSwitchConnection: (ticketId: string) => void;
  modelOptions: string[];
  currentModel: string | null;
  switchingModel: boolean;
  onSwitchModel: (model: string) => void;
  effortOptions?: string[];
  currentEffort?: string | null;
  onSwitchEffort?: (effort: string) => void;
  onPickWorkingDirectory: () => void;
  onDraftKeyDown?: (e: KeyboardEvent<HTMLTextAreaElement>) => boolean;
  onPasteImages?: (files: File[]) => void;
  runtimeControls?: ReactNode;
  connectionLocked?: boolean;
  runtimeLocked?: boolean;
  fillHeight?: boolean;
  paneHeight?: number | null;
  paneRef?: Ref<HTMLDivElement>;
  showBlockerBanner?: boolean;
  emptyTranscript?: boolean;
  modelMenuOpenNonce?: number;
  commandSearchOpen?: boolean;
  commandIndex?: number;
  actionContext?: ChatActionContext;
  extraActions?: ChatActionDef[];
  onRunAction?: (action: ChatActionDef) => void;
  onHoverCommandIndex?: (index: number) => void;
  runtimeEnabled?: boolean;
  steer?: boolean;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const firstBlocker = blockers[0] ?? null;
  const hiddenBlocked = firstBlocker?.kind === 'hiddenAgents' ||
    active.agentIds.some((id) => hiddenIds.has(id));
  const action = composerPrimaryAction({
    hasDraft: Boolean(draft.trim()),
    blocked: blockers.length > 0,
    sending,
    canSteer: Boolean(onSteer),
    canQueue: Boolean(onQueueAfterTurn),
  });
  const footerControl = composerFooterControl({ sending, action });
  const shortcutKind = composerShortcutKind({
    blocked: blockers.length > 0,
    sending,
    canSteer: Boolean(onSteer),
    canQueue: Boolean(onQueueAfterTurn),
  });
  const stopCopy = t(composerStopMessageKey(canceling));
  const stopTitle = composerStopTitle({ canceling, stopLabel: stopCopy });
  const footerSlotClass = 'h-8 w-8 shrink-0 rounded-full';
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const modelMenuDisabled = sending || connectionLocked || switchingProvider || switchingModel;
  const currentEffortHint = currentEffort ? chatEffortHint(currentEffort, t) : null;
  useEffect(() => {
    if (!modelMenuOpenNonce) return;
    if (modelMenuDisabled || modelOptions.length === 0) return;
    setModelMenuOpen(true);
  }, [modelMenuDisabled, modelMenuOpenNonce, modelOptions.length]);

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
    if (!focusNonce) return;
    textareaRef.current?.focus();
  }, [focusNonce]);

  useEffect(() => {
    const onResize = () => syncTextareaHeight();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [syncTextareaHeight]);

  const textareaDisabled = hiddenBlocked;
  const keepComposerFocus = useCallback(() => {
    if (!composerShouldRestoreFocus({ textareaDisabled })) return;
    textareaRef.current?.focus();
  }, [textareaDisabled]);
  const submitComposer = useCallback(() => {
    if (action === 'steer') onSteer?.();
    else if (action === 'queue') onQueueAfterTurn?.();
    else if (action === 'send') onSend();
    keepComposerFocus();
    requestAnimationFrame(keepComposerFocus);
  }, [action, keepComposerFocus, onQueueAfterTurn, onSend, onSteer]);
  const droppedImages = useCallback((files: FileList | null | undefined) => {
    if (!onPasteImages) return false;
    const images = Array.from(files ?? []).filter((file) => file.type.startsWith('image/'));
    if (images.length === 0) return false;
    onPasteImages(images);
    return true;
  }, [onPasteImages]);
  const sendHint = firstBlocker
    ? blockerCopy(t, firstBlocker).text
    : t(composerSubmitMessageKey(action));
  const selectedAgent = active.agentIds[0] ?? '';
  const approveFooter = autoApproveFooter(t, active.allowDangerous, active.agentIds[0] ?? null);
  const pickerEmpty = chatAgentPickerEmptyKind({
    agentsReady,
    rowCount: pickerRows.length,
  });
  const pickerEmptyCopy = pickerEmpty ? chatAgentPickerEmptyCopy(t, pickerEmpty) : null;
  const compactSecondary = composerCompactSecondary({ emptyTranscript });
  const capabilityHint = composerCapabilityHint(t, {
    runtimeEnabled,
    steer,
    sending,
  });
  const shortcutHint = t(composerShortcutMessageKey(shortcutKind));
  const hoverHint = composerHoverHint(shortcutHint, capabilityHint);
  const showHintRow = composerShowsHintRow({ emptyTranscript });
  const connectionHint = composerConnectionTooltip({
    label: connectionView.label,
    subtitle: connectionView.subtitle,
    caption: connectionCaption,
  });

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
      {firstBlocker && showBlockerBanner ? (
        <BlockerNotice
          blocker={firstBlocker}
          onGoAgents={() => navigate('/agents')}
          onGoConnections={() =>
            navigate(primaryAgent ? `/connections?agent=${primaryAgent}` : '/connections')
          }
          onPickWorkingDirectory={onPickWorkingDirectory}
          onRetryStatus={onRetryStatus}
        />
      ) : null}
      <div
        ref={paneRef}
        data-help="chat-composer"
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
            'px-4 pb-2 pt-3 text-body leading-relaxed outline-none placeholder:text-muted',
            'disabled:cursor-not-allowed disabled:opacity-60',
          )}
          style={
            fillHeight
              ? undefined
              : { minHeight: COMPOSER_TEXTAREA_MIN_PX, maxHeight: COMPOSER_TEXTAREA_MAX_PX }
          }
          placeholder={composerInvitePlaceholder(t, { emptyTranscript })}
          rows={1}
          value={draft}
          disabled={textareaDisabled}
          enterKeyHint="send"
          aria-keyshortcuts="Enter"
          title={hoverHint}
          onChange={(e) => setDraft(e.target.value)}
          onInput={syncTextareaHeight}
          onKeyDown={(e) => {
            const edit = composerNativeEditChord({
              key: e.key,
              code: e.nativeEvent.code,
              metaKey: e.metaKey,
              ctrlKey: e.ctrlKey,
              altKey: e.altKey,
              shiftKey: e.shiftKey,
            });
            if (edit === 'selectAll') {
              e.preventDefault();
              e.currentTarget.select();
              return;
            }
            if (edit) return;
            if (onDraftKeyDown?.(e)) return;
            if (!composerEnterShouldSubmit({
              key: e.key,
              shiftKey: e.shiftKey,
              composing: e.nativeEvent.isComposing,
              keyCode: e.nativeEvent.keyCode,
            })) {
              return;
            }
            e.preventDefault();
            if (action) submitComposer();
          }}
          onPaste={(e) => {
            if (droppedImages(e.clipboardData?.files)) e.preventDefault();
          }}
          onDragOver={(e) => {
            if (!onPasteImages) return;
            const hasImage = Array.from(e.dataTransfer?.items ?? []).some((item) =>
              item.kind === 'file' && item.type.startsWith('image/'),
            );
            if (!hasImage) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }}
          onDrop={(e) => {
            if (droppedImages(e.dataTransfer?.files)) e.preventDefault();
          }}
          aria-label={t('chat.composer.inputAria')}
        />
        {onRunAction ? (
          <ChatActionMenu
            draft={draft}
            commandOpen={commandSearchOpen}
            selectedIndex={commandIndex}
            actionContext={actionContext}
            extraActions={extraActions}
            onRun={onRunAction}
            onHoverIndex={onHoverCommandIndex}
            anchorRef={textareaRef}
          />
        ) : null}
        <ChatQueuedFollowUpList
          items={queuedFollowUps}
          onCancelItem={onCancelQueuedFollowUp}
          onCancelAll={onClearQueuedFollowUp}
        />
        <div className="flex items-center justify-between gap-2 px-4 pb-1" data-composer-shortcut="">
          <div className="min-w-0">
            {showHintRow ? (
              <p className="text-meta text-muted">{shortcutHint}</p>
            ) : null}
          </div>
          <ChatShortcutsHelp />
        </div>
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/50 px-2 py-1.5">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={sending || runtimeLocked}
                title={runtimeLocked ? t('chat.runtimeOps.sessionLocked') : undefined}
                className="max-w-36"
              >
                {active.agentIds[0] && <AgentLogo agentId={active.agentIds[0]} size="sm" />}
                <span className="truncate">{agentPickerLabel}</span>
                <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56">
              <DropdownMenuLabel>{t('chat.composer.selectAgent')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={selectedAgent}
                onValueChange={(id) => onSelectAgent(id as AgentKey)}
              >
                {pickerRows.map((row) => (
                  <DropdownMenuRadioItem
                    key={row.id}
                    value={row.id}
                    disabled={sending || !row.selectable}
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
            <Hint label={connectionHint}>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={
                    !primaryAgent ||
                    sending ||
                    connectionLocked ||
                    switchingProvider ||
                    Boolean(primaryAgent && hiddenIds.has(primaryAgent))
                  }
                  className={compactSecondary ? 'max-w-[6.5rem]' : 'max-w-28'}
                  aria-label={connectionHint || t('chat.composer.switchConnection')}
                >
                  <span className="min-w-0 truncate">{connectionView.label}</span>
                  <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
                </Button>
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
            <Hint label={`${t('chat.composer.switchModel')} · ${t('chat.composer.shortcutOpenModel')}`}>
              <DropdownMenu open={modelMenuOpen} onOpenChange={setModelMenuOpen}>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={modelMenuDisabled}
                    className="max-w-36"
                    data-help="chat-model"
                    aria-label={t('chat.composer.switchModel')}
                    aria-keyshortcuts="Control+Shift+I"
                  >
                    <span className="min-w-0 truncate">
                      {currentModel
                        ? chatModelDisplayName(currentModel, t)
                        : t('chat.composer.switchModel')}
                    </span>
                    <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
                  </Button>
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
                        disabled={sending || connectionLocked || switchingModel}
                      >
                        <span className="truncate">{chatModelDisplayName(model, t)}</span>
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            </Hint>
          ) : null}

          {effortOptions.length > 0 && onSwitchEffort ? (
            <Hint label={currentEffortHint ?? t('chat.runtimeOps.effort')}>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={sending || connectionLocked || switchingProvider || switchingModel}
                    data-help="chat-composer-more"
                    aria-label={t('chat.composer.moreOptions')}
                    title={t('chat.composer.moreOptions')}
                  >
                    <MoreHorizontal className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-56">
                  <DropdownMenuLabel>{t('chat.runtimeOps.effort')}</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  <DropdownMenuRadioGroup
                    value={currentEffort ?? ''}
                    onValueChange={(id) => onSwitchEffort(id)}
                  >
                    {effortOptions.map((effort) => {
                      const hint = chatEffortHint(effort, t);
                      return (
                        <DropdownMenuRadioItem
                          key={effort}
                          value={effort}
                          disabled={sending || connectionLocked || switchingModel}
                          data-help="chat-effort"
                        >
                          <span className="flex min-w-0 flex-1 items-baseline justify-between gap-3">
                            <span className="truncate">{chatEffortLabel(effort, t)}</span>
                            {hint ? <span className="shrink-0 text-meta text-muted">{hint}</span> : null}
                          </span>
                        </DropdownMenuRadioItem>
                      );
                    })}
                  </DropdownMenuRadioGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            </Hint>
          ) : null}

          {runtimeControls ? (
            <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-visible">
              {runtimeControls}
            </div>
          ) : approveFooter.text ? (
            <Tip
              className={cn(
                'min-w-0 flex-1 truncate text-left text-meta leading-none',
                approveFooter.warning ? 'text-warning/50' : 'text-muted/35',
              )}
              label={approveFooter.text}
            >
              {approveFooter.text}
            </Tip>
          ) : (
            <div className="min-w-0 flex-1" />
          )}

          {footerControl === 'stop' ? (
            <Button
              type="button"
              size="icon"
              variant="dangerOutline"
              className={footerSlotClass}
              disabled={canceling}
              aria-busy={canceling}
              data-help="chat-stop"
              aria-label={stopCopy}
              aria-keyshortcuts="Escape"
              title={stopTitle}
              onClick={onCancel}
            >
              <Square className="h-3.5 w-3.5 fill-current" />
            </Button>
          ) : (
            <Button
              type="button"
              size="icon"
              variant={action ? 'default' : 'secondary'}
              className={footerSlotClass}
              disabled={!action}
              onClick={submitComposer}
              data-help="chat-send"
              aria-label={sendHint}
              title={composerHoverHint(sendHint, hoverHint)}
            >
              <ArrowUp className="h-4 w-4" />
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
  onPickWorkingDirectory,
  onRetryStatus,
}: {
  blocker: ChatSendBlocker;
  onGoAgents: () => void;
  onGoConnections: () => void;
  onPickWorkingDirectory: () => void;
  onRetryStatus?: () => void;
}) {
  const { t } = useI18n();
  const copy = blockerCopy(t, blocker);
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
          retry: onRetryStatus,
        }[blockerPrimaryTarget(blocker)]
      }
    >
      {copy.text}
    </Notice>
  );
}
