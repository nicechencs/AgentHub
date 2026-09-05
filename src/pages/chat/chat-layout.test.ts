import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('chat layout wiring', () => {
  it('keeps the main column on canvas so an empty transcript matches composer chrome', () => {
    const page = source('index.tsx');
    expect(page).toContain('flex min-w-0 flex-1 flex-col bg-canvas');
    expect(page).toContain('chatStageClass');
    expect(page).not.toContain('flex min-w-0 flex-1 flex-col bg-panel');
  });

  it('grows the composer textarea with a shared cap and panel-colored shell', () => {
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('[field-sizing:content]');
    expect(composer).toContain('COMPOSER_TEXTAREA_MIN_PX');
    expect(composer).toContain('COMPOSER_TEXTAREA_MAX_PX');
    expect(composer).toContain('composerTextareaMeasuredStyle');
    expect(composer).toContain('composerUsesCssFieldSizing');
    expect(composer).toContain('rounded-composer border border-border bg-panel');
  });

  it('derives the transcript surface from whether any turns exist', () => {
    expect(source('ChatTranscript.tsx')).toContain(
      'chatTranscriptSurfaceClass(turns.length > 0)',
    );
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
});
