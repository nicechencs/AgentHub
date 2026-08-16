import { useNavigate } from 'react-router-dom';
import { MessagesSquare } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { ChatComposer } from './ChatComposer';
import { ChatSessionHeader } from './ChatSessionHeader';
import { ChatSessionRail } from './ChatSessionRail';
import { ChatSettingsDialog } from './ChatSettingsDialog';
import { ChatTranscript } from './ChatTranscript';
import { useChatPage } from './use-chat-page';

export default function ChatPage() {
  const page = useChatPage();
  const navigate = useNavigate();

  if (page.error && page.conversations.length === 0 && !page.listLoading) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ErrorState error={page.error} onRetry={page.retryLoad} />
      </div>
    );
  }

  if (!page.listLoading && page.conversations.length === 0 && !page.hasUsableAgent) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <EmptyState
          icon={MessagesSquare}
          title="还没有可对话的 Agent"
          description="安装或取消隐藏 Agent 后即可开始"
          action={
            <Button
              size="sm"
              variant="secondary"
              className="mt-2"
              onClick={() => navigate('/agents')}
            >
              去 Agents 页
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
        hasUsableAgent={page.hasUsableAgent}
        deleteConfirmId={page.deleteConfirmId}
        onToggleRail={() => page.setRailOpen(false)}
        onNewChat={() => void page.handleNewChat()}
        onFocus={page.focusConversation}
        onRequestDelete={page.setDeleteConfirmId}
        onCancelDelete={() => page.setDeleteConfirmId(null)}
        onConfirmDelete={() => void page.confirmDelete()}
      />

      <section className="relative flex min-w-0 flex-1 flex-col bg-panel">
        <ChatSessionHeader
          active={page.active}
          railOpen={page.railOpen}
          hiddenIds={page.hiddenIds}
          onExpandRail={() => page.setRailOpen(true)}
          onRename={page.renameTitle}
          onOpenSettings={() => page.setSettingsOpen(true)}
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
              'shrink-0 border-t border-border/60 bg-canvas pb-4 pt-2',
              pageRhythm.chatChromeX,
            )}
          >
            <div className="mx-auto w-full max-w-3xl">
              <ChatComposer
                draft={page.draft}
                setDraft={page.setDraft}
                sending={page.sendingHere}
                active={page.active}
                installed={page.installed}
                providers={page.providers}
                primaryAgent={page.primaryAgent}
                agentPickerLabel={page.agentPickerLabel}
                modelPickerLabel={page.modelPickerLabel}
                modelPickerSubtitle={page.modelPickerSubtitle}
                switchingProvider={page.switchingProvider}
                hiddenIds={page.hiddenIds}
                blockers={page.blockers}
                connectionCaption={page.connectionCaption}
                onSend={() => void page.handleSend()}
                onCancel={() => void page.cancelSending()}
                onToggleAgent={(id) => void page.toggleConversationAgent(id)}
                onSwitchProvider={(id) => void page.handleSwitchProvider(id)}
                onOpenSettings={() => page.setSettingsOpen(true)}
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
