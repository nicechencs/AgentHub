import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { MessagesSquare } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { SideSplitFrame } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Notice } from '@/components/shared/Notice';
import { isMarkdownFilePath } from '@/components/shared/MarkdownView';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { StorageKey } from '@/lib/storage-key';
import { cn } from '@/lib/utils';
import {
  chatComposerChoiceOptions,
  chatShowsRuntimeRequestPanels,
  kiroChatBannerCopy,
  kiroChatStance,
} from './chat-kiro-model';
import {
  chatEscapeShouldCancel,
  chatKeyTargetIsField,
  chatModKShouldFocusHistory,
  chatModNShouldStartNewChat,
  chatQuestionShouldOpenShortcuts,
  chatMainColumnClass,
  chatStageClass,
} from './chat-model';
import { chatModShiftIShouldOpenModel } from './chat-model-labels';
import { formatChatSessionRecord } from './chat-format';
import { chatBusySendMode, grokLegacyContinueKind } from './chat-grok-follow-up';
import { ChatMarkdownPreviewPanel } from './ChatMarkdownPreviewPanel';
import {
  chatPreviewCanBack,
  chatPreviewPath,
  openChatPreviewRoot,
  popChatPreview,
  pushChatPreview,
  type ChatPreviewTarget,
} from './chat-preview-model';
import { ChatRuntimeExtras } from './ChatRuntimeExtras';
import { ChatTurnOutcomeBanner } from './ChatTurnOutcomeBanner';
import { ChatComposer } from './ChatComposer';
import { ChatSessionHeader } from './ChatSessionHeader';
import { ChatSessionRail } from './ChatSessionRail';
import { ChatSettingsDialog } from './ChatSettingsDialog';
import { ChatShortcutsDialog } from './ChatShortcutsDialog';
import { ChatTranscript } from './ChatTranscript';
import { ChatRuntimeRequests } from './ChatRuntimeRequests';
import { useChatComposerSplit } from './use-chat-composer-split';
import { useChatPage } from './use-chat-page';

export default function ChatPage() {
  const page = useChatPage();
  const busySend = chatBusySendMode({
    sending: page.sendingHere,
    runtimeEnabled: page.runtime?.enabled,
    steer: page.runtimeOps.steer,
    runId: page.runtime?.runId,
    phase: page.runtime?.phase,
    queued: Boolean(page.queuedFollowUp),
  });
  const split = useChatComposerSplit();
  const preview = useSideSplit<ChatPreviewTarget>({
    storageKey: StorageKey.chatPreviewWidth,
  });
  const navigate = useNavigate();
  const { t } = useI18n();
  const [modelMenuOpenNonce, setModelMenuOpenNonce] = useState(0);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const openMarkdownPreview = useCallback(
    (next: string) => {
      if (!isMarkdownFilePath(next)) return false;
      preview.open(openChatPreviewRoot(next));
      return true;
    },
    [preview.open],
  );
  const openNestedMarkdown = useCallback(
    (next: string) => {
      if (!isMarkdownFilePath(next)) return;
      preview.open(pushChatPreview(preview.target, next));
    },
    [preview.open, preview.target],
  );
  const backMarkdownPreview = useCallback(() => {
    const previous = popChatPreview(preview.target);
    if (!previous) {
      preview.close();
      return;
    }
    preview.open(previous);
  }, [preview.close, preview.open, preview.target]);

  useEffect(() => {
    preview.reset();
  }, [page.active?.id, preview.reset]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        chatModKShouldFocusHistory({
          key: e.key,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
          overlayOpen: hasEscPriorityOverlay(),
        })
      ) {
        e.preventDefault();
        page.runChatAction({
          id: 'focus-history-search',
          kind: 'local',
          keywords: [],
        });
        return;
      }
      if (
        chatModShiftIShouldOpenModel({
          key: e.key,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
          overlayOpen: hasEscPriorityOverlay(),
        })
      ) {
        e.preventDefault();
        setModelMenuOpenNonce((n) => n + 1);
        return;
      }
      if (
        chatModNShouldStartNewChat({
          key: e.key,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
          overlayOpen: hasEscPriorityOverlay(),
        })
      ) {
        e.preventDefault();
        page.runChatAction({
          id: 'new-session',
          kind: 'local',
          keywords: [],
        });
        return;
      }
      if (
        chatQuestionShouldOpenShortcuts({
          key: e.key,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          overlayOpen: hasEscPriorityOverlay(),
          typingInField: chatKeyTargetIsField(e.target),
        })
      ) {
        e.preventDefault();
        setShortcutsOpen(true);
        return;
      }
      if (
        !chatEscapeShouldCancel({
          key: e.key,
          sending: page.sendingHere,
          canceling: page.cancelingHere,
          previewOpen: preview.expanded || preview.mounted,
          overlayOpen: hasEscPriorityOverlay(),
          defaultPrevented: e.defaultPrevented,
          composing: e.isComposing,
        })
      ) {
        return;
      }
      e.preventDefault();
      void page.cancelSending();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    page.cancelSending,
    page.cancelingHere,
    page.runChatAction,
    page.sendingHere,
    preview.expanded,
    preview.mounted,
  ]);

  if (page.error && page.conversations.length === 0 && !page.listLoading) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ChatShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
        <ErrorState error={page.error} onRetry={page.retryLoad} />
      </div>
    );
  }

  if (
    !page.listLoading &&
    page.conversations.length === 0 &&
    page.agentsReady &&
    !page.hasUsableAgent
  ) {
    return (
      <div className="flex h-full items-center justify-center p-6" data-help="chat-empty">
        <ChatShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
        <EmptyState
          icon={MessagesSquare}
          title={t('chat.page.emptyTitle')}
          description={t('chat.page.emptyDesc')}
          actionLabel={t('chat.page.goAgents')}
          onAction={() => navigate('/agents')}
        />
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 bg-canvas">
      <ChatSessionRail
        open={page.railOpen}
        listLoading={page.listLoading}
        groups={page.railGroups}
        conversations={page.conversations}
        filteredCount={page.filteredCount}
        query={page.railQuery}
        onQueryChange={page.setRailQuery}
        activeId={page.activeId}
        sendingConversationIds={page.sendingConversationIds}
        agentsReady={page.agentsReady}
        hasUsableAgent={page.hasUsableAgent}
        deleteConfirmId={page.deleteConfirmId}
        onToggleRail={() => page.setRailOpen(false)}
        onNewChat={() => void page.handleNewChat()}
        onFocus={page.focusConversation}
        onRequestDelete={page.setDeleteConfirmId}
        onCancelDelete={() => page.setDeleteConfirmId(null)}
        onConfirmDelete={() => void page.confirmDelete()}
        searchFocusNonce={page.searchFocusNonce}
        historyRevealNonce={page.historyRevealNonce}
      />

      <div ref={preview.splitRef} className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <section className="relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-card border border-border bg-canvas">
        <ChatSessionHeader
          active={page.active}
          railOpen={page.railOpen}
          recordText={formatChatSessionRecord(page.turns, t('common.you'))}
          onExpandRail={() => page.setRailOpen(true)}
          onRename={page.renameTitle}
          onOpenSettings={() => page.setSettingsOpen(true)}
          onPickWorkingDirectory={() => void page.pickWorkingDirectory()}
          runtimeLocked={page.runtimeLocked || page.sendingHere}
        />

        <div
          className={cn(chatStageClass, pageRhythm.chatChromeX, 'relative')}
          data-chat-stage
        >
          <div
            ref={split.splitRef}
            className={cn(chatMainColumnClass, 'flex min-h-0 flex-1 flex-col')}
          >
            <ChatTranscript
              active={page.active}
              turns={page.turns}
              processMap={page.processMap}
              listLoading={page.listLoading}
              messagesLoading={page.messagesLoading}
              messagesError={page.messagesError}
              onRetryMessages={page.retryMessages}
              sending={page.sending}
              retryDisabled={page.blockers.length > 0}
              scrollRef={page.transcriptRef}
              bottomRef={page.bottomRef}
              onScroll={page.onTranscriptScroll}
              onRetry={() => void page.retryLast()}
              onOpenLocal={openMarkdownPreview}
              onPickStarter={page.runChatAction}
              firstBlocker={page.blockers[0] ?? null}
              onBlockerAction={(target) => {
                if (target === 'agents') navigate('/agents');
                else if (target === 'connections') {
                  navigate(page.primaryAgent ? `/connections?agent=${page.primaryAgent}` : '/connections');
                }
                else if (target === 'pick-directory') void page.pickWorkingDirectory();
                else void page.refreshAgents().catch(() => {});
              }}
            />
            {chatShowsRuntimeRequestPanels(page.primaryAgent) && page.runtime?.pendingRequests.length ? (
              <ChatRuntimeRequests
                requests={page.runtime.pendingRequests}
                onReply={(request, decision, answers) => page.submitRuntimeRequest(request, decision, answers)}
              />
            ) : null}

            {page.active && (
              <>
                {page.turnOutcome ? (
                  <ChatTurnOutcomeBanner
                    outcome={page.turnOutcome}
                    retryDisabled={page.blockers.length > 0 || page.sending}
                    onRetry={() => void page.retryLast()}
                    onRestoreDraft={() => page.setDraft(page.turnOutcome?.prompt ?? '')}
                  />
                ) : null}
                {(() => {
                  const stance = kiroChatStance(page.primaryAgent);
                  if (!stance?.showBanner) return null;
                  const copy = kiroChatBannerCopy(t);
                  return (
                    <Notice tone="info" className="mb-2">
                      <div className="space-y-1" data-help="chat-kiro-oneshot">
                        <p className="font-medium text-primary">{copy.title}</p>
                        <p className="text-meta text-secondary">{copy.detail}</p>
                      </div>
                    </Notice>
                  );
                })()}
                <div
                  role="separator"
                  aria-orientation="horizontal"
                  aria-label={t('chat.composer.resizeAria')}
                  aria-valuenow={split.paneHeight ?? undefined}
                  aria-valuemin={split.valuemin}
                  tabIndex={0}
                  onPointerDown={split.onResizeStart}
                  onDoubleClick={split.resetHeight}
                  onKeyDown={split.onSeparatorKeyDown}
                  className="relative z-10 h-2 shrink-0 cursor-row-resize bg-transparent outline-none"
                />
                {(() => {
                  const kind = grokLegacyContinueKind({
                    agentId: page.primaryAgent,
                    runtimeEnabled: page.runtime?.enabled,
                    runtimeReady: page.runtime != null,
                    hasMessages: page.messages.length > 0,
                    nativeSessionId: page.active.nativeSessionId,
                  });
                  if (!kind) return null;
                  return (
                    <Notice tone="warning" className="mb-2">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <p className="min-w-0 text-meta text-secondary">
                          {kind === 'continue'
                            ? t('chat.composer.legacyContinueHint')
                            : t('chat.composer.legacyNewChatHint')}
                        </p>
                        {kind === 'continue' ? (
                          <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            disabled={page.sendingHere}
                            onClick={() => void page.continueLegacyGrok()}
                          >
                            {t('chat.composer.legacyContinueAction')}
                          </Button>
                        ) : (
                          <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            disabled={!page.actionContext.newChatAllowed}
                            onClick={() => void page.handleNewChat()}
                          >
                            {t('chat.composer.legacyNewChatAction')}
                          </Button>
                        )}
                      </div>
                    </Notice>
                  );
                })()}
                <ChatComposer
                  draft={page.draft}
                  setDraft={page.setDraft}
                  sending={page.sendingHere}
                  canceling={page.cancelingHere}
                  active={page.active}
                  connectionOptions={page.connectionOptions}
                  primaryAgent={page.primaryAgent}
                  agentPickerLabel={page.agentPickerLabel}
                  connectionView={page.connectionView}
                  switchingProvider={page.switchingProvider}
                  hiddenIds={page.hiddenIds}
                  pickerRows={page.pickerRows}
                  agentsReady={page.agentsReady}
                  blockers={page.blockers}
                  showBlockerBanner={page.turns.length > 0}
                  connectionCaption={page.connectionCaption}
                  walletError={page.walletError}
                  onRetryWallet={() => void page.reloadWallet()}
                  onRetryStatus={() => void page.refreshAgents().catch(() => {})}
                  onSend={() => void page.handleSend()}
                  onSteer={
                    busySend === 'steer'
                      ? () => {
                          const value = page.draft;
                          void page.steerRuntime(value).then((ok) => {
                            if (ok) page.setDraft('');
                          }).catch(() => {});
                        }
                      : undefined
                  }
                  onQueueAfterTurn={
                    busySend === 'queue'
                      ? () => void page.handleSend()
                      : undefined
                  }
                  queuedFollowUp={page.queuedFollowUp}
                  queuedFollowUpCount={page.queuedFollowUpCount}
                  onClearQueuedFollowUp={page.clearQueuedFollowUp}
                  focusNonce={page.composerFocusNonce}
                  modelMenuOpenNonce={modelMenuOpenNonce}
                  onOpenShortcuts={() => setShortcutsOpen(true)}
                  onCancel={() => void page.cancelSending()}
                  onSelectAgent={(id) => void page.selectConversationAgentId(id)}
                  onSwitchConnection={(id) => void page.handleSwitchConnection(id)}
                  modelOptions={
                    page.runtime?.enabled
                      ? []
                      : chatComposerChoiceOptions(page.primaryAgent, page.modelOptions)
                  }
                  currentModel={page.runtime?.enabled ? null : page.currentModel}
                  switchingModel={page.runtime?.enabled ? false : page.switchingModel}
                  onSwitchModel={(id) => {
                    if (page.runtime?.enabled) return;
                    void page.handleSwitchModel(id);
                  }}
                  effortOptions={
                    page.runtime?.enabled
                      ? []
                      : chatComposerChoiceOptions(page.primaryAgent, page.effortOptions)
                  }
                  currentEffort={page.runtime?.enabled ? null : page.currentEffort}
                  onSwitchEffort={(id) => {
                    if (page.runtime?.enabled) return;
                    void page.handleSwitchEffort(id);
                  }}
                  onPickWorkingDirectory={() => void page.pickWorkingDirectory()}
                  onDraftKeyDown={page.handleComposerKeyDown}
                  onPasteImages={
                    page.runtime?.enabled && page.runtimeOps.imageInput
                      ? (files) => void page.runtimeOps.pasteImages(files)
                      : undefined
                  }
                  connectionLocked={page.connectionLocked}
                  runtimeLocked={page.runtimeLocked}
                  runtimeControls={
                    page.runtime?.enabled && !kiroChatStance(page.primaryAgent) ? (
                      <ChatRuntimeExtras
                        enabled
                        inline
                        modelMenuOpenNonce={modelMenuOpenNonce}
                        draft={page.draft}
                        commandSearchOpen={page.commandSearchOpen}
                        commandIndex={page.commandIndex}
                        actionContext={page.actionContext}
                        extraActions={page.runtimeCommandActions}
                        onRunAction={page.runChatAction}
                        onHoverCommandIndex={page.setCommandIndex}
                        models={page.runtimeOps.models}
                        settings={page.runtimeOps.settings}
                        frozen={page.runtimeOps.frozen}
                        frozenReason={page.primaryAgent === 'kiro' ? t('chat.kiro.settingsLocked') : undefined}
                        catalogLoading={page.runtimeOps.loading}
                        efforts={page.runtimeOps.currentEfforts}
                        onSwitchModel={(id) => void page.runtimeOps.switchModel(id)}
                        onSwitchEffort={(id) => void page.runtimeOps.switchEffort(id)}
                        images={page.runtimeOps.images}
                        imageInput={page.runtimeOps.imageInput}
                        onAddImages={() => void page.runtimeOps.addImages()}
                        onRemoveImage={page.runtimeOps.removeImage}
                        onPasteImages={(files) => void page.runtimeOps.pasteImages(files)}
                        extensions={page.runtimeOps.extensions}
                        selectedSkillIds={page.runtimeOps.selectedSkillIds}
                        onToggleSkill={page.runtimeOps.toggleSkill}
                        agentId={page.primaryAgent}
                        showSkillPicker={page.primaryAgent !== 'codex'}
                      />
                    ) : undefined
                  }
                  fillHeight={split.paneHeight != null}
                  paneHeight={split.paneHeight}
                  paneRef={split.composerPaneRef}
                />
              </>
            )}
          </div>
        </div>

        <ChatShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
        <ChatSettingsDialog
          open={page.settingsOpen}
          onOpenChange={page.setSettingsOpen}
          active={page.active}
          dangerConfirm={page.dangerConfirm}
          onDangerConfirmChange={page.setDangerConfirm}
          onPatch={(patch) => void page.patchActive(patch)}
          runtimeLocked={page.runtimeLocked || page.sendingHere}
        />
      </section>
        <SideSplitFrame split={preview} resizeAria={t('chat.preview.resizeAria')}>
          {preview.target ? (
            <ChatMarkdownPreviewPanel
              path={chatPreviewPath(preview.target)}
              cwd={page.active?.cwd ?? ''}
              open={preview.expanded}
              width={preview.paneWidth}
              canBack={chatPreviewCanBack(preview.target)}
              onBack={backMarkdownPreview}
              onClose={preview.close}
              onOpenLocal={openNestedMarkdown}
              className="h-full min-w-0"
            />
          ) : null}
        </SideSplitFrame>
      </div>
    </div>
  );
}
