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
import { onChatNativeShortcut } from '@/lib/api/chat';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { StorageKey } from '@/lib/storage-key';
import { processKey } from '@/lib/chat-process';
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
  chatPageShortcutAction,
  chatMainColumnClass,
  chatStageClass,
  composerNativeEditChord,
} from './chat-model';
import { subscribeChatShortcutKeydown } from './chat-shortcuts';
import { chatModShiftIShouldOpenModel } from './chat-model-labels';
import { formatChatSessionRecord, type TurnGroup } from './chat-format';
import { chatBusySendMode, grokLegacyContinueKind } from './chat-grok-follow-up';
import { ChatMarkdownPreviewPanel } from './ChatMarkdownPreviewPanel';
import { ChatProcessInspectPanel } from './ChatProcessInspectPanel';
import {
  chatPreviewCanBack,
  chatPreviewPath,
  isChatFilePreview,
  isChatProcessInspect,
  openChatPreviewRoot,
  openChatProcessInspect,
  popChatPreview,
  pushChatPreview,
  type ChatInspectTarget,
  type ChatProcessInspectTarget,
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
import { ChatHostTerminals } from './ChatHostTerminals';
import { ChatPlanBar } from './ChatPlanBar';
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
    queued: page.queuedFollowUpCount > 0,
  });
  const split = useChatComposerSplit();
  const preview = useSideSplit<ChatInspectTarget>({
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
  const openProcessInspect = useCallback(
    (turn: number, agent: string) => {
      preview.open(openChatProcessInspect(turn, agent));
    },
    [preview.open],
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
        chatKeyTargetIsField(e.target) &&
        composerNativeEditChord({
          key: e.key,
          code: e.code,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
        })
      ) {
        return;
      }
      const overlayOpen = hasEscPriorityOverlay();
      const action = chatPageShortcutAction({
        key: e.key,
        code: e.code,
        metaKey: e.metaKey,
        ctrlKey: e.ctrlKey,
        altKey: e.altKey,
        shiftKey: e.shiftKey,
        overlayOpen,
        target: e.target,
      });
      if (action === 'history') {
        e.preventDefault();
        e.stopPropagation();
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
          overlayOpen,
        })
      ) {
        e.preventDefault();
        e.stopPropagation();
        setModelMenuOpenNonce((n) => n + 1);
        return;
      }
      if (action === 'newChat') {
        e.preventDefault();
        e.stopPropagation();
        page.runChatAction({
          id: 'new-session',
          kind: 'local',
          keywords: [],
        });
        return;
      }
      if (action === 'overview') {
        e.preventDefault();
        e.stopPropagation();
        setShortcutsOpen(true);
        return;
      }
      if (
        !chatEscapeShouldCancel({
          key: e.key,
          sending: page.sendingHere,
          canceling: page.cancelingHere,
          previewOpen: preview.expanded || preview.mounted,
          overlayOpen,
          defaultPrevented: e.defaultPrevented,
          composing: e.isComposing,
        })
      ) {
        return;
      }
      e.preventDefault();
      void page.cancelSending();
    };
    return subscribeChatShortcutKeydown(onKey);
  }, [
    page.cancelSending,
    page.cancelingHere,
    page.runChatAction,
    page.sendingHere,
    preview.expanded,
    preview.mounted,
  ]);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void onChatNativeShortcut((action) => {
      if (cancelled || action !== 'newChat') return;
      page.runChatAction({
        id: 'new-session',
        kind: 'local',
        keywords: [],
      });
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unsub = fn;
      })
      .catch(() => {
        // Browser mock / unavailable: page keydown still handles Chromium.
      });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [page.runChatAction]);

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
        firstUserContentById={page.firstUserContentById}
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
              hideLastTurnRetry={Boolean(page.turnOutcome)}
              onOpenLocal={openMarkdownPreview}
              onOpenProcess={openProcessInspect}
              onCloseProcess={preview.close}
              inspectProcess={
                isChatProcessInspect(preview.target) && preview.expanded ? preview.target : null
              }
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
            {chatShowsRuntimeRequestPanels(page.runtime?.enabled) && page.runtime?.pendingRequests.length ? (
              <ChatRuntimeRequests
                agentId={page.primaryAgent}
                requests={page.runtime.pendingRequests}
                onReply={(request, decision, answers) => page.submitRuntimeRequest(request, decision, answers)}
              />
            ) : null}
            {page.runtime?.hostTerminals?.length ? (
              <ChatHostTerminals
                terminals={page.runtime.hostTerminals}
                onKill={page.killHostTerminal}
              />
            ) : null}

            {page.active && (
              <>
                {page.cwdMissing ? (
                  <Notice tone="warning" className="mb-2">
                    <div
                      className="flex flex-wrap items-center justify-between gap-2"
                      data-help="chat-cwd-missing"
                    >
                      <div className="min-w-0 space-y-1">
                        <p className="font-medium text-primary">{t('chat.cwd.missing')}</p>
                        <p className="text-meta text-secondary">{t('chat.cwd.missingDetail')}</p>
                      </div>
                      <div className="flex shrink-0 flex-wrap gap-2">
                        {page.fallbackCwd ? (
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => void page.pickWorkingDirectory(page.fallbackCwd)}
                          >
                            {t('chat.cwd.useProjectDir')}
                          </Button>
                        ) : null}
                        <Button
                          size="sm"
                          variant="secondary"
                          onClick={() => void page.pickWorkingDirectory()}
                        >
                          {t('chat.cwd.rebind')}
                        </Button>
                      </div>
                    </div>
                  </Notice>
                ) : null}
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
                <ChatPlanBar plan={page.runtime?.plan} />
                <ChatComposer
                  draft={page.draft}
                  setDraft={page.setDraft}
                  sending={page.sendingHere}
                  canceling={page.cancelingHere}
                  active={page.active}
                  connectionOptions={page.connectionOptions}
                  primaryAgent={page.primaryAgent}
                  runtimeEnabled={Boolean(page.runtime?.enabled)}
                  steer={page.runtimeOps.steer}
                  agentPickerLabel={page.agentPickerLabel}
                  connectionView={page.connectionView}
                  switchingProvider={page.switchingProvider}
                  hiddenIds={page.hiddenIds}
                  pickerRows={page.pickerRows}
                  agentsReady={page.agentsReady}
                  blockers={page.blockers}
                  showBlockerBanner={page.turns.length > 0}
                  emptyTranscript={page.turns.length === 0}
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
                  queuedFollowUps={page.queuedFollowUps}
                  onCancelQueuedFollowUp={page.cancelQueuedFollowUp}
                  onClearQueuedFollowUp={page.clearQueuedFollowUp}
                  focusNonce={page.composerFocusNonce}
                  modelMenuOpenNonce={modelMenuOpenNonce}
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
                  commandSearchOpen={page.commandSearchOpen}
                  commandIndex={page.commandIndex}
                  actionContext={page.actionContext}
                  extraActions={page.runtimeCommandActions}
                  onRunAction={page.runChatAction}
                  onHoverCommandIndex={page.setCommandIndex}
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
                        compactSecondary={page.turns.length === 0}
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
          {isChatFilePreview(preview.target) ? (
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
          ) : isChatProcessInspect(preview.target) ? (
            <ChatProcessInspectPanel
              view={page.processMap[processKey(preview.target.turn, preview.target.agent)]}
              messageStatus={inspectMessageStatus(page.turns, preview.target)}
              exitCode={inspectExitCode(page.turns, preview.target)}
              open={preview.expanded}
              onClose={preview.close}
              width={preview.paneWidth}
              className="h-full min-w-0"
            />
          ) : null}
        </SideSplitFrame>
      </div>
    </div>
  );
}

function inspectAgentMessage(turns: TurnGroup[], target: ChatProcessInspectTarget) {
  const group = turns.find((item) => item.turn === target.turn);
  return group?.agents.find((message) => (message.agentId ?? 'claude') === target.agent);
}

function inspectMessageStatus(turns: TurnGroup[], target: ChatProcessInspectTarget) {
  return inspectAgentMessage(turns, target)?.status;
}

function inspectExitCode(turns: TurnGroup[], target: ChatProcessInspectTarget) {
  return inspectAgentMessage(turns, target)?.exitCode ?? null;
}
