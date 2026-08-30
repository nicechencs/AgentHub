import { describe, expect, it } from 'vitest';
import { pageEdgePx, pageRhythm } from '@/components/layout/page-rhythm';

describe('pageRhythm (docs/ui-design.md §2 / §3.1)', () => {
  it('keeps the app chrome as two rounded columns on an 8px canvas gutter', () => {
    expect(pageRhythm.shell).toContain('p-2');
    expect(pageRhythm.shell).toContain('gap-2');
    expect(pageRhythm.shellNav).toContain('rounded-card');
    expect(pageRhythm.shellMain).toContain('rounded-card');
    expect(pageRhythm.shellNav).toContain('overflow-hidden');
    expect(pageRhythm.shellMain).toContain('overflow-hidden');
  });

  it('uses the edge-inset column: fill the main pane at 12px, no max-width cap', () => {
    expect(pageRhythm.pageShell).toContain('px-3');
    expect(pageRhythm.pageShell).toContain('py-3');
    expect(pageRhythm.pageShell).not.toContain('max-w-');
    expect(pageRhythm.pageShell).not.toContain('mx-auto');
    expect(pageRhythm.workbenchX).toBe('px-3');
    expect(pageEdgePx.x).toBe(12);
  });

  it('uses one centered reading column for Chat messages and Settings forms', () => {
    expect(pageRhythm.readingColumn).toBe('mx-auto w-full max-w-3xl');
  });

  it('keeps Chat chrome at 16px and the workbench at 12px', () => {
    expect(pageRhythm.chatChromeX).toBe('px-4');
    expect(pageRhythm.workbenchX).toBe('px-3');
  });

  it('pulls the split-list scrollbar off the separator instead of padding the cards', () => {
    expect(pageRhythm.workbenchXSplit).toBe('pl-3 pr-2 mr-2');
    expect(pageRhythm.workbenchXSplit).not.toContain('px-');
  });

  it('locks page titles to one type, one-line title+meta, and 12px inset', () => {
    expect(pageRhythm.workbenchHeader).toContain('px-3');
    expect(pageRhythm.workbenchHeader).toContain('pt-3');
    expect(pageRhythm.workbenchHeader).not.toContain('py-3');
    expect(pageRhythm.chromeRow).toContain('min-h-10');
    expect(pageRhythm.chromeActions).toContain('ml-auto');
    expect(pageRhythm.lead).toContain('mb-3');
    expect(pageRhythm.pageShell).toContain('px-3');
    expect(pageRhythm.pageShell).toContain('py-3');
    expect(pageRhythm.pageTitle).toBe('text-title font-semibold tracking-tight text-primary');
    expect(pageRhythm.pageTitleMeta).toContain('text-meta');
    expect(pageRhythm.pageTitleMeta).toContain('text-secondary');
    expect(pageRhythm.pageTitleBlock).toBe('flex min-w-0 items-baseline gap-2.5');
    expect(pageRhythm.topChrome).toBe('h-10');
  });

  it('starts workbench body flush under the 12px header, with 12px bottom inset', () => {
    expect(pageRhythm.workbenchY).toBe('pb-3');
    expect(pageRhythm.workbenchY).not.toMatch(/pt-|py-/);
    expect(pageRhythm.workbenchPadT).toBe('pt-3');
    expect(pageEdgePx.previewY).toBe(12);
  });

  it('separates page sections from nav eyebrows', () => {
    expect(pageRhythm.section).toBe('mt-6');
    expect(pageRhythm.sectionEyebrow).toContain('text-meta');
    expect(pageRhythm.sectionEyebrow).toContain('uppercase');
    expect(pageRhythm.sectionEyebrow).not.toContain('text-title');
  });
});
