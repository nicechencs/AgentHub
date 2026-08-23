import { useNavigate } from 'react-router-dom';
import { MessagesSquare } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { chatComposerChromeClass } from './chat-model';
import { ChatComposer } from './ChatComposer';
import { ChatSessionHeader } from './ChatSessionHeader';
import { ChatSessionRail } from './ChatSessionRail';
import { ChatSettingsDialog } from './ChatSettingsDialog';
import { ChatTranscript } from './ChatTranscript';
import { useChatPage } from './use-chat-page';

export default function ChatPage() {
  const page = useChatPage();
  const navigate = useNavigate();
  const { t } = useI18n();

  if (page.error && page.conversations.length === 0 && !page.listLoading) {
    return (
      <div className="flex h-full items-center justify-center p-6">
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
      <div className="flex h-full items-center justify-center p-6">
        <EmptyState
          icon={MessagesSquare}
          title={t('chat.page.emptyTitle')}
          description={t('chat.page.emptyDesc')}
          action={
            <Button
              size="sm"
              variant="secondary"
              className="mt-2"
              onClick={() => navigate('/agents')}
            >
              {t('chat.page.goAgents')}
            </Button>
          }
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
        sendingConversationId={page.sendingConversationId}
        agentsReady={page.agentsReady}
        hasUsableAgent={page.hasUsableAgent}
        deleteConfirmId={page.deleteConfirmId}
        onToggleRail={() => page.setRailOpen(false)}
        onNewChat={() => void page.handleNewChat()}
        onFocus={page.focusConversation}
        onRequestDelete={page.setDeleteConfirmId}
        onCancelDelete={() => page.setDeleteConfirmId(null)}
        onConfirmDelete={() => void page.confirmDelete()}
      />

      <section className="relative flex min-w-0 flex-1 flex-col bg-canvas">
        <ChatSessionHeader
          active={page.active}
          railOpen={page.railOpen}
          hiddenIds={page.hiddenIds}
          onExpandRail={() => page.setRailOpen(true)}
          onRename={page.renameTitle}
          onOpenSettings={() => page.setSettingsOpen(true)}
          onPickWorkingDirectory={() => void page.pickWorkingDirectory()}
        />

        <ChatTranscript
          active={page.active}
          turns={page.turns}
          processMap={page.processMap}
          listLoading={page.listLoading}
          messagesLoading={page.messagesLoading}
          sending={page.sending}
          retryDisabled={page.blockers.length > 0}
          scrollRef={page.transcriptRef}
          bottomRef={page.bottomRef}
          onScroll={page.onTranscriptScroll}
          onRetry={() => void page.retryLast()}
        />

        {page.active && (
          <div
            className={cn(
              chatComposerChromeClass(page.turns.length > 0),
              pageRhythm.chatChromeX,
            )}
          >
            <div className="mx-auto w-full max-w-3xl">
              <ChatComposer
                draft={page.draft}
                setDraft={page.setDraft}
                sending={page.sendingHere}
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
                connectionCaption={page.connectionCaption}
                onSend={() => void page.handleSend()}
                onCancel={() => void page.cancelSending()}
                onSelectAgent={(id) => void page.selectConversationAgentId(id)}
                onSwitchConnection={(id) => void page.handleSwitchConnection(id)}
                onOpenSettings={() => page.setSettingsOpen(true)}
                onPickWorkingDirectory={() => void page.pickWorkingDirectory()}
                onFocusConversation={page.focusConversation}
              />
            </div>
          </div>
        )}

        <ChatSettingsDialog
          open={page.settingsOpen}
          onOpenChange={page.setSettingsOpen}
          active={page.active}
          dangerConfirm={page.dangerConfirm}
          onDangerConfirmChange={page.setDangerConfirm}
          onPatch={(patch) => void page.patchActive(patch)}
        />
      </section>
    </div>
  );
}
