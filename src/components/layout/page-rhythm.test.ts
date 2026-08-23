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

  it('uses the edge-inset column: fill the main pane at 18px, no max-width cap', () => {
    expect(pageRhythm.pageShell).toContain('px-[18px]');
    expect(pageRhythm.pageShell).toContain('py-[18px]');
    expect(pageRhythm.pageShell).not.toContain('max-w-');
    expect(pageRhythm.pageShell).not.toContain('mx-auto');
    expect(pageRhythm.workbenchX).toBe('px-[18px]');
    expect(pageEdgePx.x).toBe(18);
  });

  it('uses one centered reading column for Chat messages and Settings forms', () => {
    expect(pageRhythm.readingColumn).toBe('mx-auto w-full max-w-3xl');
  });

  it('keeps Chat chrome at 16px and the workbench at 18px', () => {
    expect(pageRhythm.chatChromeX).toBe('px-4');
    expect(pageRhythm.workbenchX).toBe('px-[18px]');
  });

  it('locks page titles to one type, two-line height, and 18px inset', () => {
    expect(pageRhythm.workbenchHeader).toContain('px-[18px]');
    expect(pageRhythm.workbenchHeader).toContain('py-[18px]');
    expect(pageRhythm.pageShell).toContain('px-[18px]');
    expect(pageRhythm.pageShell).toContain('py-[18px]');
    expect(pageRhythm.pageTitle).toBe('text-title font-semibold tracking-tight text-primary');
    expect(pageRhythm.pageTitleBlock).toBe('min-h-10');
  });

  it('starts workbench body flush under the 18px header, with 18px bottom inset', () => {
    expect(pageRhythm.workbenchY).toBe('pb-[18px]');
    expect(pageRhythm.workbenchY).not.toMatch(/pt-|py-/);
    expect(pageEdgePx.previewY).toBe(18);
  });

  it('separates page sections from nav eyebrows', () => {
    expect(pageRhythm.section).toBe('mt-6');
    expect(pageRhythm.sectionEyebrow).toContain('text-meta');
    expect(pageRhythm.sectionEyebrow).toContain('uppercase');
    expect(pageRhythm.sectionEyebrow).not.toContain('text-title');
  });
});
