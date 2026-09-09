import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  type KeyboardEvent,
  type ReactNode,
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
import type { AgentKey, Conversation } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  composerEnterShouldSubmit,
  composerPrimaryAction,
  composerQueuedFollowUpView,
  composerShortcutKind,
  composerShortcutMessageKey,
  composerShouldRestoreFocus,
  composerShowsSubmitButton,
  composerStopMessageKey,
  composerSubmitMessageKey,
} from './chat-composer-model';
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
import { kiroChatComposerPlaceholder } from './chat-kiro-model';

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
  queuedFollowUp = null,
  queuedFollowUpCount = 0,
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
  queuedFollowUp?: string | null;
  queuedFollowUpCount?: number;
  onClearQueuedFollowUp?: () => void;
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
  const showSubmit = composerShowsSubmitButton({ sending, action });
  const shortcutKind = composerShortcutKind({
    blocked: blockers.length > 0,
    sending,
    canSteer: Boolean(onSteer),
    canQueue: Boolean(onQueueAfterTurn),
  });
  const queueView = composerQueuedFollowUpView(queuedFollowUp, queuedFollowUpCount);
  const stopCopy = t(composerStopMessageKey(canceling));
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
          placeholder={kiroChatComposerPlaceholder(
            t,
            primaryAgent,
            t('chat.composer.placeholder'),
          )}
          rows={1}
          value={draft}
          disabled={textareaDisabled}
          enterKeyHint="send"
          aria-keyshortcuts="Enter"
          onChange={(e) => setDraft(e.target.value)}
          onInput={syncTextareaHeight}
          onKeyDown={(e) => {
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
        {queueView ? (
          <div
            className="mx-3 mb-1 flex items-center gap-2 rounded-btn bg-subtle px-2 py-1"
            role="status"
            aria-live="polite"
          >
            <p className="min-w-0 flex-1 truncate text-meta text-secondary">
              {t('chat.composer.queuedCount', { count: queueView.count })}
              {' · '}
              {t('chat.composer.queuedHint')}
              {'：'}
              {queueView.preview}
            </p>
            {onClearQueuedFollowUp ? (
              <Button type="button" size="sm" variant="ghost" onClick={onClearQueuedFollowUp}>
                {t('chat.composer.clearQueuedFollowUp')}
              </Button>
            ) : null}
          </div>
        ) : null}
        <p className="px-4 pb-1 text-meta text-muted" data-composer-shortcut="">
          {t(composerShortcutMessageKey(shortcutKind))}
        </p>
        <div className="flex shrink-0 items-center gap-1.5 border-t border-border/50 px-2 py-2">
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
            <Hint label={connectionCaption ?? undefined}>
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
                  className="max-w-32"
                  aria-label={connectionCaption ?? t('chat.composer.switchConnection')}
                >
                  <span className="min-w-0 truncate">
                    {connectionView.label}
                    {connectionView.subtitle ? (
                      <span className="text-muted"> · {connectionView.subtitle}</span>
                    ) : null}
                  </span>
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
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
              disabled={sending || connectionLocked || switchingProvider || switchingModel}
                  className="max-w-40"
                  aria-label={t('chat.composer.switchModel')}
                >
                  <span className="min-w-0 truncate">
                    {currentModel || t('chat.composer.switchModel')}
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
                      <span className="truncate">{model}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}

          {effortOptions.length > 0 && onSwitchEffort ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={sending || connectionLocked || switchingProvider || switchingModel}
                  className="max-w-32"
                  aria-label={t('chat.runtimeOps.effort')}
                >
                  <span className="min-w-0 truncate">
                    {currentEffort || t('chat.runtimeOps.effort')}
                  </span>
                  <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-48">
                <DropdownMenuLabel>{t('chat.runtimeOps.effort')}</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuRadioGroup
                  value={currentEffort ?? ''}
                  onValueChange={(id) => onSwitchEffort(id)}
                >
                  {effortOptions.map((effort) => (
                    <DropdownMenuRadioItem
                      key={effort}
                      value={effort}
                      disabled={sending || connectionLocked || switchingModel}
                    >
                      <span className="truncate">{effort}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}

          {runtimeControls ? (
            <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
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

          {sending ? (
            <Button
              type="button"
              size="sm"
              variant="dangerOutline"
              className="shrink-0"
              disabled={canceling}
              aria-busy={canceling}
              data-help="chat-stop"
              aria-label={stopCopy}
              title={stopCopy}
              onClick={onCancel}
            >
              <Square className="h-3.5 w-3.5" />
              {stopCopy}
            </Button>
          ) : null}
          {showSubmit ? (
            <Button
              type="button"
              size="icon"
              variant={action ? 'default' : 'secondary'}
              className="h-8 w-8 shrink-0 rounded-full"
              disabled={!action}
              onClick={submitComposer}
              data-help="chat-send"
              aria-label={sendHint}
              title={sendHint}
            >
              <SendHorizontal className="h-4 w-4" />
            </Button>
          ) : null}
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
