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
  it('keeps rounded columns on the canvas gutter and a square flush status bar', () => {
    expect(pageRhythm.shell).toContain('flex-col');
    expect(pageRhythm.shell).toContain('bg-canvas');
    expect(pageRhythm.shell).not.toContain(pageCanvasTw.p);
    expect(pageRhythm.shell).not.toContain('gap-');
    expect(pageRhythm.shellBody).toContain(pageCanvasTw.x);
    expect(pageRhythm.shellBody).toContain(pageCanvasTw.t);
    expect(pageRhythm.shellBody).toContain('pb-1');
    expect(pageRhythm.shellBody).not.toContain(pageCanvasTw.b);
    expect(pageRhythm.shellNav).toContain('rounded-card');
    expect(pageRhythm.shellMain).toContain('rounded-card');
    expect(pageRhythm.statusBar).toContain('h-8');
    expect(pageRhythm.statusBar).toContain('border-t');
    expect(pageRhythm.statusBar).toContain('px-6');
    expect(pageRhythm.statusBar).not.toContain('rounded-card');
    expect(pageRhythm.statusBar).not.toContain('shadow-xs');
    expect(pageRhythm.shellNav).toContain('overflow-hidden');
    expect(pageRhythm.shellMain).toContain('overflow-hidden');
  });

  it('keeps splitters hidden until hover, centered in an 8px hit', () => {
    expect(pageEdge.separator).toBe(8);
    expect(pageRhythm.sash).toContain('w-2 shrink-0');
    expect(pageRhythm.sash).toContain('after:w-px');
    expect(pageRhythm.sash).toContain('after:left-1/2');
    expect(pageRhythm.sash).toContain('after:bg-transparent');
    expect(pageRhythm.sash).toContain('hover:after:bg-accent');
    expect(pageRhythm.sash).not.toContain('after:bg-border');
    expect(pageRhythm.sash).not.toContain('absolute inset-y-0 right-0');
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
    expect(pageRhythm.workbenchXSplit).toBe(`${pageInsetTw.l} ${pageInsetTw.mr}`);
    expect(pageRhythm.workbenchXSplit).not.toContain('px-');
    expect(pageRhythm.workbenchXSplit).not.toContain(pageInsetTw.r);
    expect(pageRhythm.chatChromeX).toBe(pageChatTw.x);
    expect(pageRhythm.chatChromeX).toBe(pageRhythm.workbenchX);
  });

  it('keeps Chat on the reading column and Dashboard/Settings forms on the overview column', () => {
    expect(pageRhythm.readingColumn).toBe('mx-auto w-full max-w-3xl');
    expect(pageRhythm.overviewColumn).toBe('mx-auto w-full max-w-6xl');
  });

  it('keeps the current page inset at 12px (change pageEdge.inset to retune)', () => {
    expect(pageEdge.inset).toBe(12);
    expect(pageEdge.canvas).toBe(12);
    expect(pageEdge.chat).toBe(pageEdge.inset);
    expect(pageEdge.chat).toBe(12);
    expect(pageInsetTw.x).toBe('px-3');
    expect(pageChatTw.x).toBe('px-3');
  });

  it('locks page titles to one type, one-line title+meta, and the shared inset', () => {
    expect(pageRhythm.chromeRow).toContain('min-h-10');
    expect(pageRhythm.chromeActions).toContain('ml-auto');
    expect(pageRhythm.lead).toContain('mb-3');
    expect(pageRhythm.pageTitle).toBe('text-title font-semibold tracking-tight text-primary');
    expect(pageRhythm.pageTitleMeta).toContain('text-meta');
    expect(pageRhythm.pageTitleMeta).toContain('text-secondary');
    expect(pageRhythm.pageTitleBlock).toBe('flex min-w-0 items-baseline gap-2.5');
    expect(pageRhythm.topChrome).toBe('h-11');
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
