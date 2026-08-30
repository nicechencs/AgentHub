import { describe, expect, it } from 'vitest';
import {
  pageCanvasTw,
  pageChatTw,
  pageEdge,
  pageEdgePx,
  pageInsetTw,
  pageRhythm,
} from '@/components/layout/page-rhythm';

describe('pageRhythm (docs/ui/design-system.md §3)', () => {
  it('keeps the app chrome as two rounded columns on the canvas gutter', () => {
    expect(pageRhythm.shell).toContain(pageCanvasTw.p);
    expect(pageRhythm.shell).toContain(pageCanvasTw.gap);
    expect(pageRhythm.shellNav).toContain('rounded-card');
    expect(pageRhythm.shellMain).toContain('rounded-card');
    expect(pageRhythm.shellNav).toContain('overflow-hidden');
    expect(pageRhythm.shellMain).toContain('overflow-hidden');
  });

  it('derives every page inset class and pixel from pageEdge.inset', () => {
    expect(pageEdgePx.x).toBe(pageEdge.inset);
    expect(pageEdgePx.previewY).toBe(pageEdge.inset);
    expect(pageEdgePx.separator).toBe(pageEdge.separator);
    expect(pageRhythm.workbenchX).toBe(pageInsetTw.x);
    expect(pageRhythm.workbenchPadT).toBe(pageInsetTw.t);
    expect(pageRhythm.workbenchY).toBe(pageInsetTw.b);
    expect(pageRhythm.pageShell).toContain(pageInsetTw.x);
    expect(pageRhythm.pageShell).toContain(pageInsetTw.y);
    expect(pageRhythm.pageShell).not.toContain('max-w-');
    expect(pageRhythm.pageShell).not.toContain('mx-auto');
    expect(pageRhythm.workbenchHeader).toContain(pageInsetTw.x);
    expect(pageRhythm.workbenchHeader).toContain(pageInsetTw.t);
    expect(pageRhythm.workbenchHeader).not.toContain(pageInsetTw.y);
    expect(pageRhythm.workbenchXSplit).toBe(`${pageInsetTw.l} ${pageCanvasTw.r} ${pageCanvasTw.mr}`);
    expect(pageRhythm.workbenchXSplit).not.toContain('px-');
    expect(pageRhythm.chatChromeX).toBe(pageChatTw.x);
  });

  it('uses one centered reading column for Chat messages and a left-aligned one for Settings forms', () => {
    expect(pageRhythm.readingColumn).toBe('mx-auto w-full max-w-3xl');
    expect(pageRhythm.readingStart).toBe('w-full max-w-3xl');
  });

  it('keeps the current page inset at 8px (change pageEdge.inset to retune)', () => {
    expect(pageEdge.inset).toBe(8);
    expect(pageEdge.canvas).toBe(8);
    expect(pageEdge.chat).toBe(16);
    expect(pageInsetTw.x).toBe('px-2');
    expect(pageChatTw.x).toBe('px-4');
  });

  it('locks page titles to one type, one-line title+meta, and the shared inset', () => {
    expect(pageRhythm.chromeRow).toContain('min-h-10');
    expect(pageRhythm.chromeActions).toContain('ml-auto');
    expect(pageRhythm.lead).toContain('mb-3');
    expect(pageRhythm.pageTitle).toBe('text-title font-semibold tracking-tight text-primary');
    expect(pageRhythm.pageTitleMeta).toContain('text-meta');
    expect(pageRhythm.pageTitleMeta).toContain('text-secondary');
    expect(pageRhythm.pageTitleBlock).toBe('flex min-w-0 items-baseline gap-2.5');
    expect(pageRhythm.topChrome).toBe('h-10');
  });

  it('starts workbench body with inset top/bottom and no extra py on the bottom token', () => {
    expect(pageRhythm.workbenchY).not.toMatch(/pt-|py-/);
    expect(pageRhythm.workbenchY).toBe(pageInsetTw.b);
    expect(pageRhythm.workbenchPadT).toBe(pageInsetTw.t);
  });

  it('separates page sections from nav eyebrows', () => {
    expect(pageRhythm.section).toBe('mt-6');
    expect(pageRhythm.sectionEyebrow).toContain('text-meta');
    expect(pageRhythm.sectionEyebrow).toContain('uppercase');
    expect(pageRhythm.sectionEyebrow).not.toContain('text-title');
  });
});
