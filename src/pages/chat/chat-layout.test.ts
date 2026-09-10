import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('chat layout wiring', () => {
  it('keeps the main column on canvas so an empty transcript matches composer chrome', () => {
    const page = source('index.tsx');
    expect(page).toContain('flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-card border border-border bg-canvas');
    expect(page).toContain('chatStageClass');
    expect(page).not.toContain('flex min-w-0 flex-1 flex-col bg-panel');
    expect(source('ChatMessageBubble.tsx')).toContain('formatChatDisplayContent');
    expect(source('ChatTranscript.tsx')).toContain('overflow-x-hidden overflow-y-auto');
  });

  it('lets Escape stop an in-flight turn', () => {
    expect(source('index.tsx')).toContain('chatEscapeShouldCancel');
    expect(source('index.tsx')).toContain("e.key");
    expect(source('ChatComposer.tsx')).toContain('composerStopMessageKey');
    expect(source('ChatComposer.tsx')).toContain('composerStopTitle');
    expect(source('ChatComposer.tsx')).toContain('data-help="chat-stop"');
    expect(source('ChatComposer.tsx')).toContain('aria-keyshortcuts="Escape"');
    expect(source('use-chat-page-send.ts')).toContain('composerKeepsStoppingAfterCancel');
    expect(source('use-chat-page-send.ts')).toContain('composerCancelingVisible');
  });

  it('opens the model menu from Ctrl/Cmd+Shift+I and labels models in plain language', () => {
    expect(source('index.tsx')).toContain('chatModShiftIShouldOpenModel');
    expect(source('index.tsx')).toContain('modelMenuOpenNonce');
    expect(source('ChatRuntimeExtras.tsx')).toContain('chatModelDisplayName');
    expect(source('ChatRuntimeExtras.tsx')).toContain('chatEffortHint');
    expect(source('ChatRuntimeExtras.tsx')).toContain('data-help="chat-model"');
    expect(source('ChatRuntimeExtras.tsx')).toContain('data-help="chat-composer-cluster"');
    expect(source('ChatRuntimeExtras.tsx')).toContain('max-w-36');
    expect(source('ChatComposer.tsx')).toContain('px-2 py-1.5');
    expect(source('ChatComposer.tsx')).toContain('chatModelDisplayName');
    expect(source('ChatComposer.tsx')).toContain('chatEffortHint');
    expect(translate('zh', 'chat.runtimeOps.effortHintHigh')).toBe('可能更慢');
    expect(translate('zh', 'chat.composer.shortcutOpenModel')).toBe('Ctrl+Shift+I');
  });

  it('starts a new chat from Ctrl/Cmd+N and opens a shortcut overview', () => {
    const page = source('index.tsx');
    expect(page).toContain('chatPageShortcutAction');
    expect(page).toContain('subscribeChatShortcutKeydown');
    expect(page).toContain('onChatNativeShortcut');
    expect(page).toContain("id: 'new-session'");
    expect(page).toContain('ChatShortcutsDialog');
    expect(source('chat-shortcuts.ts')).toContain("addEventListener('keydown', wrapped, true)");
    expect(source('use-chat-page.ts')).toContain('chatModNShouldStartNewChat');
    expect(source('ChatComposer.tsx')).toContain('ChatShortcutsHelp');
    expect(source('ChatShortcutsHelp.tsx')).toContain('data-help="chat-shortcuts"');
    expect(source('ChatShortcutsHelp.tsx')).toContain('aria-keyshortcuts="?"');
    expect(source('ChatShortcutsHelp.tsx')).toContain('aria-expanded={open}');
    expect(source('ChatShortcutsHelp.tsx')).toContain('data-help="chat-shortcuts-popover"');
    expect(source('ChatShortcutsDialog.tsx')).toContain('ChatShortcutOverview');
    expect(source('ChatShortcutOverview.tsx')).toContain('CHAT_SHORTCUT_ROWS');
    expect(source('ChatShortcutOverview.tsx')).toContain('EnterKeyMark');
    expect(source('ChatShortcutOverview.tsx')).not.toMatch(/>Enter</);
    expect(source('ChatSessionRail.tsx')).toContain('aria-keyshortcuts="Control+N"');
    expect(source('ChatSessionRail.tsx')).toContain('data-help="chat-session-title"');
    expect(translate('zh', 'chat.shortcuts.open')).toBe('快捷键');
    expect(translate('en', 'chat.shortcuts.open')).toBe('Shortcuts');
  });

  it('keeps one footer slot that switches Send and Stop', () => {
    expect(source('index.tsx')).toContain('chatBusySendMode');
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('composerPrimaryAction');
    expect(composer).toContain('composerFooterControl');
    expect(composer).toContain("footerControl === 'stop'");
    expect(composer).toContain('data-help="chat-send"');
    expect(composer).toContain('data-help="chat-stop"');
    expect(composer).toContain('footerSlotClass');
    expect(composer).toContain('h-8 w-8 shrink-0 rounded-full');
    expect(composer).not.toContain('{sending ? (');
    expect(composer).not.toContain('{showSubmit ? (');
  });

  it('names Enter / Shift+Enter, shows the queue, and restores composer focus', () => {
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('composerEnterShouldSubmit');
    expect(composer).toContain('composerShortcutMessageKey');
    expect(composer).toContain('data-composer-shortcut');
    expect(composer).toContain('ChatQueuedFollowUpList');
    expect(composer).toContain('queuedFollowUps');
    expect(source('ChatQueuedFollowUpList.tsx')).toContain('chat.composer.queuedCount');
    expect(source('ChatQueuedFollowUpList.tsx')).toContain('cancelQueuedItem');
    expect(composer).toContain('keepComposerFocus');
    expect(composer).toContain('enterKeyHint="send"');
    expect(composer).toContain("t('chat.composer.moreOptions')");
    expect(composer).toContain('flex min-w-0 flex-1 items-center gap-1.5 overflow-visible');
    expect(composer).not.toContain('flex-col justify-center gap-1.5');
    expect(composer).not.toContain('chat.actions.menu');
    expect(source('use-chat-page.ts')).toContain('composerEnterShouldSubmit');
    expect(source('index.tsx')).toContain('queuedFollowUpCount');
    expect(translate('zh', 'chat.composer.shortcutSend')).toContain('Enter 发送');
    expect(translate('zh', 'chat.composer.stopping')).toBe('正在停止');
  });

  it('opens markdown files in a right-hand preview pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('useSideSplit');
    expect(page).toContain('SideSplitFrame');
    expect(page).toContain('ChatMarkdownPreviewPanel');
    expect(page).toContain('isMarkdownFilePath');
    expect(source('ChatMarkdownPreviewPanel.tsx')).toContain('readMarkdownPreview');
    expect(source('ChatMarkdownPreviewPanel.tsx')).toContain('chat.preview.back');
    expect(source('index.tsx')).toContain('pushChatPreview');
  });

  it('opens the turn process in the same right-hand pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('ChatProcessInspectPanel');
    expect(page).toContain('openChatProcessInspect');
    expect(page).toContain('onOpenProcess');
    expect(source('ChatMessageBubble.tsx')).toContain('data-help="chat-process-chip"');
    expect(source('ChatMessageBubble.tsx')).not.toContain('ChatProcessPanel');
    expect(source('ChatProcessInspectPanel.tsx')).toContain('ChatProcessPanel');
  });

  it('grows the composer textarea with a shared cap and panel-colored shell', () => {
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('[field-sizing:content]');
    expect(composer).toContain('COMPOSER_TEXTAREA_MIN_PX');
    expect(composer).toContain('COMPOSER_TEXTAREA_MAX_PX');
    expect(composer).toContain('composerTextareaMeasuredStyle');
    expect(composer).toContain('composerUsesCssFieldSizing');
    expect(composer).toContain('rounded-composer border border-border bg-panel');
    expect(composer).toContain('text-body leading-relaxed');
    expect(composer).not.toContain('leading-[1.45]');
    expect(composer).toContain('composerNativeEditChord');
    expect(composer).toContain("edit === 'selectAll'");
    expect(composer).toContain('e.currentTarget.select()');
    expect(source('index.tsx')).toContain('composerNativeEditChord');
    expect(source('index.tsx')).toContain('chatKeyTargetIsField');
  });

  it('loosens chat bubble reading line-height without changing bubble chrome', () => {
    const bubble = source('ChatMessageBubble.tsx');
    expect(bubble).toContain('text-body leading-relaxed text-primary');
    expect(bubble).toContain('text-body leading-relaxed text-danger');
    expect(bubble).toContain('rounded-composer bg-subtle');
  });

  it('paints the transcript surface on the scroller, not a message card', () => {
    const transcript = source('ChatTranscript.tsx');
    expect(transcript).toContain('chatTranscriptSurfaceClass');
    expect(transcript).toContain('data-chat-transcript');
    expect(transcript).not.toContain('chatTranscriptSurfaceClass(turns.length > 0)');
    expect(source('index.tsx')).toContain('data-chat-stage');
  });

  it('shows empty-session starter cards that fill the composer', () => {
    const transcript = source('ChatTranscript.tsx');
    expect(transcript).toContain('chatStarterActions');
    expect(transcript).toContain('onPickStarter');
    expect(transcript).toContain('emptyTranscriptCopy');
    expect(transcript).toContain('text-display');
    expect(source('chat-empty-state.ts')).toContain('chat.transcript.startersHint');
    expect(source('chat-empty-state.ts')).not.toContain('chat.transcript.identity');
    expect(source('chat-empty-state.ts')).not.toContain('chat.transcript.firstMessage');
    expect(source('ChatTranscript.tsx')).toContain('emptyStarterChipHint');
    expect(source('ChatComposer.tsx')).toContain('focusNonce');
    expect(source('index.tsx')).toContain('focusNonce={page.composerFocusNonce}');
    expect(transcript).toContain('firstBlocker');
    expect(transcript).not.toContain('variant="default"');
    expect(source('index.tsx')).toContain('onPickStarter={page.runChatAction}');
    expect(source('index.tsx')).toContain('firstBlocker={page.blockers[0] ?? null}');
    expect(source('index.tsx')).toContain('chat-cwd-missing');
    expect(source('index.tsx')).toContain('showBlockerBanner={page.turns.length > 0}');
    expect(source('index.tsx')).toContain('emptyTranscript={page.turns.length === 0}');
    expect(source('index.tsx')).toContain('compactSecondary={page.turns.length === 0}');
    expect(source('ChatComposer.tsx')).toContain('showBlockerBanner');
  });

  it('keeps the transcript white column on the same max-w-3xl as the composer', () => {
    expect(source('index.tsx')).toContain('chatMainColumnClass');
    expect(source('index.tsx')).toContain('chatStageClass');
    expect(source('index.tsx')).toContain('pageRhythm.chatChromeX');
    expect(source('ChatSessionHeader.tsx')).toContain('pageRhythm.chatChromeX');
    expect(source('ChatTranscript.tsx')).not.toContain('pageRhythm.chatChromeX');
    expect(source('ChatTranscript.tsx')).not.toContain('px-6');
    expect(source('ChatRuntimeRequests.tsx')).not.toContain('max-w-3xl');
    expect(source('ChatRuntimeRequests.tsx')).not.toContain('px-4');
  });

  it('hides the splitter in an 8px gutter between transcript and composer', () => {
    const page = source('index.tsx');
    expect(page).toContain('useChatComposerSplit');
    expect(page).toContain('role="separator"');
    expect(page).toContain('aria-orientation="horizontal"');
    expect(page).toContain('cursor-row-resize');
    expect(page).toContain('h-2 shrink-0 cursor-row-resize');
    expect(page).toContain('bg-transparent');
    expect(page).not.toContain('after:bg-border');
    expect(page).not.toContain('hover:after:bg-accent');
    expect(page).not.toContain('-my-2');
    expect(page).not.toContain('flex min-h-0 flex-1 flex-col gap-4');
  });

  it('lets a dragged composer pane fill leftover height', () => {
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('fillHeight');
    expect(composer).toContain('min-h-0 flex-1');
    expect(composer).toContain('[field-sizing:content]');
    expect(composer.indexOf('<BlockerNotice')).toBeLessThan(composer.indexOf('ref={paneRef}'));
  });

  it('puts the auto-approve hint to the left of send, muted and meta-sized', () => {
    const composer = source('ChatComposer.tsx');
    const hintAt = composer.indexOf('approveFooter.text');
    const sendAt = composer.indexOf('<ArrowUp');
    expect(hintAt).toBeGreaterThan(0);
    expect(sendAt).toBeGreaterThan(hintAt);
    expect(composer).toContain('text-muted/35');
    expect(composer).toContain('text-left text-meta leading-none');
    expect(composer).not.toContain('mt-2 shrink-0 text-center text-meta');
  });

  it('puts a routes-style sash between history and the conversation', () => {
    const rail = source('ChatSessionRail.tsx');
    expect(rail).toContain('NavResizeHandle');
    expect(rail).toContain('useNavWidth');
    expect(rail).toContain('CHAT_RAIL_WIDTH');
    expect(rail).toContain('StorageKey.chatRailWidth');
    expect(rail).toContain("t('chat.rail.resize')");
    expect(rail).toContain('rounded-card border border-border');
    expect(rail).toContain('bg-canvas');
    expect(rail).toContain('justify-between');
    expect(rail).toContain('border-b border-border');
    expect(rail).not.toContain('pageRhythm.shellNav');
    expect(rail).not.toContain('bg-panel');
    expect(rail).not.toContain('border-r border-border');
    expect(rail).not.toContain("'w-60'");
  });

  it('titles the history rail, collapses beside the title, and creates chats above search', () => {
    const rail = source('ChatSessionRail.tsx');
    const titleAt = rail.indexOf("t('chat.rail.historyTitle')");
    const collapseAt = rail.indexOf("t('chat.rail.collapseHistory')");
    const newAt = rail.indexOf("t('chat.rail.newChat')");
    const searchAt = rail.indexOf("t('chat.rail.searchPlaceholder')");
    const listAt = rail.indexOf('overflow-y-auto');
    expect(titleAt).toBeGreaterThan(0);
    expect(collapseAt).toBeGreaterThan(titleAt);
    expect(newAt).toBeGreaterThan(collapseAt);
    expect(searchAt).toBeGreaterThan(newAt);
    expect(listAt).toBeGreaterThan(searchAt);
    expect(rail).toContain('conversationRailHintView');
    expect(rail).toContain('data-help="chat-session-hint-title"');
    expect(rail).toContain('whitespace-pre-wrap break-all');
    expect(rail).toContain('[overflow-wrap:anywhere]');
    expect(rail).toContain('conversationRailMarkColor');
    expect(rail).toContain('conversationRailSelectedFill');
    expect(rail).not.toContain('bg-accent-subtle');
    expect(rail).not.toContain("'bg-active'");
    expect(rail).toContain('inset-y-1.5 left-0 w-0.5 rounded-full');
    expect(rail).toContain('AgentLogo');
    expect(rail).toContain('hint={false}');
    expect(rail).not.toContain('conversationAgentLine');
    expect(rail).not.toContain('cwdShortName');
    expect(rail).toContain('conversationTitle');
    expect(rail).not.toContain('isBlankConversationDraft');
    expect(rail).toContain("t('chat.rail.searchPlaceholder')");
    expect(source('chat-model.ts')).toContain('isBlankConversationDraft');
    expect(source('chat-model.ts')).toContain("t('chat.rail.draft')");
  });

  it('keeps the list title truncated and the hover title as the full stored string', () => {
    const rail = source('ChatSessionRail.tsx');
    const model = source('chat-model.ts');
    const listTitle = rail.match(
      /className="min-w-0 flex-1 truncate" data-help="chat-session-title"/,
    );
    expect(listTitle).not.toBeNull();
    const hintTitle = rail.match(
      /className="block w-full whitespace-pre-wrap break-all \[overflow-wrap:anywhere\] \[text-overflow:clip\]"\s+data-help="chat-session-hint-title"/,
    );
    expect(hintTitle).not.toBeNull();
    expect(rail).toContain('conversationRailHintView(');
    expect(rail).toContain('firstUserContent');
    expect(rail).toContain('firstUserContentById?.[c.id] ?? c.firstUserContent');
    expect(rail).toContain('{hint.title}');
    const page = source('use-chat-page.ts');
    expect(page).toContain('firstUserContentByListedConversations');
    expect(page).toContain('mergeFirstUserContentById');
    expect(page).toContain('firstUserContentByConversation(messages)');
    expect(rail).not.toContain('title={conversation.title}');
    expect(rail).not.toMatch(/function ConversationRailHintLabel[\s\S]*AgentLogo/);
    expect(rail).not.toContain('conversationSemanticTitle');
    expect(model).toContain('conversationRailHintTitle');
    expect(model).toContain('titleFromPrompt');
    expect(model).toContain('conversationSemanticPhrase');
    expect(model).not.toMatch(
      /export function titleFromPrompt[\s\S]*TITLE_DISPLAY_CLIP/,
    );
    expect(model).not.toMatch(
      /conversationRailHintView[\s\S]*conversationSemanticTitle/,
    );
  });

  it('confirms session delete on Enter and marks the danger button with a key icon', () => {
    const rail = source('ChatSessionRail.tsx');
    expect(rail).toContain('dialogEnterShouldConfirm');
    expect(rail).toContain('aria-keyshortcuts="Enter"');
    expect(rail).toContain('EnterKeyMark');
    expect(rail).not.toMatch(/>Enter</);
    expect(rail).not.toContain('删除确认 Enter');
    expect(rail).toContain("t('chat.rail.confirmDelete')");
    expect(rail).toContain('variant="default"');
    expect(rail).toContain('data-help="chat-new"');
  });

  it('keeps history actions visible and focusable for runtime composers', () => {
    const actions = source('ChatActionMenu.tsx');
    const extras = source('ChatRuntimeExtras.tsx');
    const composer = source('ChatComposer.tsx');
    const rail = source('ChatSessionRail.tsx');
    const page = source('index.tsx');
    const hook = source('use-chat-page.ts');
    expect(actions).toContain('createPortal');
    expect(actions).toContain('role="listbox"');
    expect(actions).toContain('slashMenuFixedPosition');
    expect(actions).toContain('bg-panel');
    expect(actions).not.toContain('bg-popover');
    expect(actions).toContain('data-help="chat-slash-menu"');
    expect(actions).toContain('text-body leading-relaxed');
    expect(actions).toContain('min-h-10');
    expect(actions).toContain('max-h-80');
    expect(actions).not.toContain('text-sm');
    expect(actions).not.toContain('py-1.5 text-left text-sm');
    expect(actions).not.toContain('DropdownMenu');
    expect(actions).not.toContain('MoreHorizontal');
    expect(source('chat-actions.ts')).not.toContain("id: 'open-history'");
    expect(source('chat-actions.ts')).not.toContain("id: 'open-settings'");
    expect(source('chat-actions.ts')).not.toContain("id: 'open-agents'");
    expect(source('chat-actions.ts')).not.toContain("id: 'open-connections'");
    expect(composer).toContain('<ChatActionMenu');
    expect(composer).toContain('anchorRef={textareaRef}');
    expect(extras).not.toContain('ChatActionMenu');
    expect(extras).not.toContain('chat.actions.menu');
    expect(page).toContain('commandSearchOpen={page.commandSearchOpen}');
    expect(page).toContain('onRunAction={page.runChatAction}');
    expect(rail).toContain('historyRevealNonce');
    expect(rail).toContain('window.setTimeout');
    expect(rail).toContain('data-session-id');
    expect(page).toContain('historyRevealNonce={page.historyRevealNonce}');
    expect(hook).toContain("action.id === 'open-history'");
    expect(hook).toContain('setHistoryRevealNonce');
  });

  it('offers always-allow on runtime permission cards', () => {
    const requests = source('ChatRuntimeRequests.tsx');
    expect(requests).toContain('runtimeAllowAlwaysCopy');
    expect(requests).toContain("submit('allow_always')");
    expect(requests).toContain('chat.runtime.allowAlways');
    expect(requests).toContain('runtimeRequestTitle');
    expect(requests).toContain('always.hintKey');
    expect(requests).toContain('data-help="chat-allow-always"');
    expect(requests).toContain('runtimeFileChangePreview');
    expect(requests).toContain('fileChangePreviewHintKey');
    expect(requests).toContain('chat.runtime.fileChangePathOnly');
    expect(translate('zh', 'chat.runtime.fileChangePathOnly')).toBe('仅有路径，无内容预览');
    expect(translate('en', 'chat.runtime.fileChangePathOnly')).toBe('Path only — no content preview');
    expect(translate('zh', 'chat.runtime.fileChangePreviewEmpty')).toBe('没有路径或内容预览');
    expect(translate('en', 'chat.runtime.fileChangePreviewEmpty')).toBe('No path or content preview');
    expect(translate('zh', 'chat.runtime.fileChangeCreate')).toBe('新增文件');
    expect(translate('en', 'chat.runtime.fileChangeCreate')).toBe('Create file');
    expect(translate('zh', 'chat.runtime.fileChangeDelete')).toBe('删除文件');
    expect(translate('en', 'chat.runtime.fileChangeDelete')).toBe('Delete file');
    expect(translate('zh', 'chat.runtime.allowAlwaysHint')).toBe('仅当前这次对话，不保存');
    expect(translate('en', 'chat.runtime.allowAlwaysHint')).toBe('This conversation only, not saved');
    expect(translate('zh', 'chat.runtime.allowAlwaysHintTurn')).toBe('仅当前这次对话，不保存');
    expect(translate('en', 'chat.runtime.allowAlwaysHintTurn')).toBe(
      'This conversation only, not saved',
    );
    expect(source('ChatTurnOutcomeBanner.tsx')).toContain('turnOutcomeDetail');
    expect(source('ChatTurnOutcomeBanner.tsx')).not.toContain('draftKept');
  });

  it('shows Kiro ask-or-full permission mode in session settings and the header', () => {
    const settings = source('ChatSettingsDialog.tsx');
    const header = source('ChatSessionHeader.tsx');
    expect(settings).toContain('chat.kiro.permissionAsk');
    expect(settings).toContain('chat.kiro.permissionFull');
    expect(settings).toContain('chat.kiro.settingsLocked');
    expect(header).toContain('chat.kiro.permissionAsk');
    expect(header).toContain('chat.kiro.permissionFull');
  });

  it('wires chat capability helpers and the Kiro composer placeholder', () => {
    const page = source('index.tsx');
    expect(page).toContain('kiroChatStance');
    expect(page).toContain('kiroChatBannerCopy');
    expect(page).toContain('chatShowsRuntimeRequestPanels');
    expect(page).toContain('chatComposerChoiceOptions');
    expect(page).toContain('data-help="chat-kiro-oneshot"');
    expect(source('ChatComposer.tsx')).toContain('composerInvitePlaceholder');
    expect(source('ChatComposer.tsx')).toContain('composerCapabilityHint');
    expect(source('ChatComposer.tsx')).toContain('composerHoverHint');
    expect(source('ChatComposer.tsx')).toContain('composerShowsHintRow');
    expect(source('ChatComposer.tsx')).toContain('composerConnectionTooltip');
    expect(source('use-chat-page.ts')).toContain('kiroChatAllowsCommandSearch');
  });

  it('waits for a snapshot before warning, and offers new chat when images cannot attach', () => {
    const page = source('index.tsx');
    expect(page).toContain('runtimeReady: page.runtime != null');
    expect(page).toContain('legacyNewChatAction');
    expect(page).toContain('handleNewChat');
  });

  it('uses shared Button for chrome icons and composer chips', () => {
    const header = source('ChatSessionHeader.tsx');
    const rail = source('ChatSessionRail.tsx');
    const composer = source('ChatComposer.tsx');
    expect(header).toContain('size="icon"');
    expect(header).toContain('variant="ghost"');
    expect(header).toContain('variant="outline"');
    expect(header).toContain('data-help="chat-settings"');
    expect(header).not.toContain('hover:bg-hover hover:text-primary');
    expect(rail).toContain('size="icon"');
    expect(rail).toContain('variant="ghost"');
    expect(composer).toContain('size="sm"');
    expect(composer).toContain('variant="outline"');
    expect(composer).not.toContain('bg-subtle px-2 text-meta text-secondary hover:bg-hover');
  });

  it('opens a new chat from the OS file-manager folder handoff', () => {
    const sessions = source('use-chat-page-sessions.ts');
    expect(sessions).toContain('isChatBootstrapHandoff');
    expect(sessions).toContain('allowEnsureDefault');
    expect(sessions).toContain('restoreChatBootstrapIfUnchanged');
    expect(sessions).toContain('takeChatBootstrap');
    expect(sessions).toContain('boot.cwd');
    expect(sessions).toContain('openConversationFromSession');
    expect(sessions).toContain('boot.sessionId');
  });

  it('uses locale copy for process run details instead of internal English', () => {
    const panel = source('ChatProcessPanel.tsx');
    expect(panel).toContain("t('chat.process.runDetails')");
    expect(panel).toContain("t('chat.process.stderr')");
    expect(panel).toContain("t('chat.process.exitCode'");
    expect(panel).toContain("t('chat.process.details')");
    expect(panel).toContain('formatToolStep');
    expect(source('ChatProcessInspectPanel.tsx')).toContain('formatProcessHeadline');
    expect(source('ChatMessageBubble.tsx')).toContain('formatProcessHeadline');
    expect(panel).not.toContain('{step.name} · {step.status}');
    expect(panel).not.toContain('>stderr<');
    expect(panel).not.toContain('exit {exitCode}');
    expect(translate('zh', 'chat.process.runDetails')).toBe('运行详情');
    expect(translate('en', 'chat.process.runDetails')).toBe('Details of this run');
    expect(translate('zh', 'chat.process.stderr')).toBe('错误输出');
    expect(translate('en', 'chat.process.stderr')).toBe('Error output');
    expect(translate('en', 'chat.process.runDetails')).not.toBe('Run details');
    expect(translate('zh', 'chat.process.toolRead')).toBe('正在读取');
    expect(translate('zh', 'chat.process.toolEdit')).toBe('正在修改');
    expect(translate('zh', 'chat.process.toolRun')).toBe('正在执行');
    expect(translate('en', 'chat.process.toolRead')).toBe('Reading');
    expect(translate('en', 'chat.process.toolEdit')).toBe('Editing');
    expect(translate('en', 'chat.process.toolRun')).toBe('Running');
    expect(translate('zh', 'chat.process.details')).toBe('细节');
    expect(translate('en', 'chat.process.details')).toBe('Details');
    expect(translate('zh', 'chat.process.usage')).toBe('用量');
    expect(translate('en', 'chat.process.usage')).toBe('Usage');
    expect(source('ChatMessageBubble.tsx')).toContain('formatTurnUsageFooter');
    expect(source('ChatMessageBubble.tsx')).not.toContain('formatVisibleUsage');
    expect(source('ChatProcessPanel.tsx')).not.toContain('formatVisibleUsage');
    expect(translate('zh', 'chat.process.usageTurn')).toBe('当前轮');
    expect(translate('zh', 'chat.process.usageSession')).toBe('累计');
  });
});
