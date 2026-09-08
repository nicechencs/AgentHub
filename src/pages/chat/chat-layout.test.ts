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
  it('keeps the main column on the panel stage so an empty transcript matches composer chrome', () => {
    const page = source('index.tsx');
    expect(page).toContain('flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-panel');
    expect(page).toContain('chatStageClass');
    expect(source('ChatSessionRail.tsx')).toContain('border-r border-border bg-canvas');
    expect(source('ChatMessageBubble.tsx')).toContain('formatChatDisplayContent');
    expect(source('ChatTranscript.tsx')).toContain('overflow-x-hidden overflow-y-auto');
  });

  it('lets Escape stop an in-flight turn', () => {
    expect(source('index.tsx')).toContain('chatEscapeShouldCancel');
    expect(source('index.tsx')).toContain("e.key");
    expect(source('ChatComposer.tsx')).toContain('chat.composer.stop');
  });

  it('keeps send available while a turn is in progress', () => {
    expect(source('index.tsx')).toContain('chatBusySendMode');
    const composer = source('ChatComposer.tsx');
    const sendingAt = composer.indexOf('{sending ? (');
    const sendWhileBusyAt = composer.indexOf('data-help="chat-send"', sendingAt);
    const stopAt = composer.indexOf("t('chat.composer.stop')", sendingAt);
    expect(sendingAt).toBeGreaterThan(0);
    expect(sendWhileBusyAt).toBeGreaterThan(sendingAt);
    expect(stopAt).toBeGreaterThan(sendWhileBusyAt);
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
    expect(transcript).toContain('chat.transcript.identity');
    expect(transcript).toContain('firstBlocker');
    expect(transcript).not.toContain('variant="default"');
    expect(source('index.tsx')).toContain('onPickStarter={page.runChatAction}');
    expect(source('index.tsx')).toContain('firstBlocker={page.blockers[0] ?? null}');
    expect(source('index.tsx')).toContain('showBlockerBanner={page.turns.length > 0}');
    expect(source('ChatComposer.tsx')).toContain('showBlockerBanner');
  });

  it('keeps the transcript white column on the same max-w-3xl as the composer', () => {
    expect(source('index.tsx')).toContain('chatMainColumnClass');
    expect(source('index.tsx')).toContain('chatStageClass');
    expect(source('index.tsx')).toContain('pageRhythm.chatChromeX');
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
    const sendAt = composer.indexOf('<SendHorizontal');
    expect(hintAt).toBeGreaterThan(0);
    expect(sendAt).toBeGreaterThan(hintAt);
    expect(composer).toContain('text-muted/35');
    expect(composer).toContain('text-left text-meta leading-none');
    expect(composer).not.toContain('mt-2 shrink-0 text-center text-meta');
  });

  it('keeps history actions visible and focusable for runtime composers', () => {
    const actions = source('ChatActionMenu.tsx');
    const rail = source('ChatSessionRail.tsx');
    const page = source('index.tsx');
    const hook = source('use-chat-page.ts');
    expect(actions).toContain('createPortal');
    expect(actions).toContain('onCloseAutoFocus');
    expect(rail).toContain('historyRevealNonce');
    expect(rail).toContain('window.setTimeout');
    expect(rail).toContain('data-session-id');
    expect(page).toContain('historyRevealNonce={page.historyRevealNonce}');
    expect(hook).toContain("action.id === 'open-history'");
    expect(hook).toContain('setHistoryRevealNonce');
  });

  it('offers always-allow on runtime permission cards', () => {
    const requests = source('ChatRuntimeRequests.tsx');
    expect(requests).toContain('requestAllowsAlways');
    expect(requests).toContain("submit('allow_always')");
    expect(requests).toContain('chat.runtime.allowAlways');
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
    expect(source('ChatComposer.tsx')).toContain('kiroChatComposerPlaceholder');
    expect(source('use-chat-page.ts')).toContain('kiroChatAllowsCommandSearch');
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
  });

  it('uses locale copy for process run details instead of internal English', () => {
    const panel = source('ChatProcessPanel.tsx');
    expect(panel).toContain("t('chat.process.runDetails')");
    expect(panel).toContain("t('chat.process.stderr')");
    expect(panel).toContain("t('chat.process.exitCode'");
    expect(panel).not.toContain('>stderr<');
    expect(panel).not.toContain('exit {exitCode}');
    expect(translate('zh', 'chat.process.runDetails')).toBe('运行详情');
    expect(translate('en', 'chat.process.runDetails')).toBe('Details of this run');
    expect(translate('zh', 'chat.process.stderr')).toBe('错误输出');
    expect(translate('en', 'chat.process.stderr')).toBe('Error output');
    expect(translate('en', 'chat.process.runDetails')).not.toBe('Run details');
  });
});
