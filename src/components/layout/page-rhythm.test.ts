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
  it('keeps the app chrome flush: canvas rail, panel stage, no outer gutter', () => {
    expect(pageRhythm.shell).toBe('flex h-full min-h-0 bg-canvas');
    expect(pageRhythm.shell).not.toContain(pageCanvasTw.p);
    expect(pageRhythm.shell).not.toContain(pageCanvasTw.gap);
    expect(pageRhythm.shellNav).toContain('bg-canvas');
    expect(pageRhythm.shellNav).toContain('border-r');
    expect(pageRhythm.shellNav).not.toContain('rounded-card');
    expect(pageRhythm.shellMain).toContain('bg-panel');
    expect(pageRhythm.shellMain).not.toContain('rounded-card');
    expect(pageRhythm.shellNav).toContain('overflow-hidden');
    expect(pageRhythm.shellMain).toContain('overflow-hidden');
  });

  it('keeps the resize sash as a 4px hit with a 1px rule that accents on hover', () => {
    expect(pageEdge.separator).toBe(4);
    expect(pageRhythm.sash).toContain('w-1');
    expect(pageRhythm.sash).toContain('after:bg-border');
    expect(pageRhythm.sash).toContain('hover:after:bg-accent');
    expect(pageRhythm.sash).not.toContain('hover:bg-accent/40');
  });

  it('keeps inspect panes flush, without card radius or shadow', () => {
    expect(pageRhythm.inspectPane).toContain('bg-canvas');
    expect(pageRhythm.inspectPane).not.toContain('rounded-card');
    expect(pageRhythm.inspectPane).not.toContain('shadow-xs');
    expect(pageRhythm.inspectHeader).toContain('h-9');
    expect(pageRhythm.inspectTitle).toContain('text-body');
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

  it('uses one centered reading column for Chat messages and Settings forms', () => {
    expect(pageRhythm.readingColumn).toBe('mx-auto w-full max-w-3xl');
    expect(pageRhythm.overviewColumn).toBe('mx-auto w-full max-w-6xl');
  });

  it('keeps the current page inset at 12px (change pageEdge.inset to retune)', () => {
    expect(pageEdge.inset).toBe(12);
    expect(pageEdge.canvas).toBe(12);
    expect(pageEdge.chat).toBe(16);
    expect(pageInsetTw.x).toBe('px-3');
    expect(pageChatTw.x).toBe('px-4');
  });

  it('locks page titles to one type, one-line title+meta, and the shared inset', () => {
    expect(pageRhythm.chromeRow).toContain('min-h-9');
    expect(pageRhythm.chromeActions).toContain('ml-auto');
    expect(pageRhythm.lead).toContain('mb-3');
    expect(pageRhythm.pageTitle).toBe('text-headline font-medium tracking-tight text-primary');
    expect(pageRhythm.pageTitleMeta).toContain('text-meta');
    expect(pageRhythm.pageTitleMeta).toContain('text-secondary');
    expect(pageRhythm.pageTitleBlock).toBe('flex min-w-0 items-baseline gap-2.5');
    expect(pageRhythm.topChrome).toBe('h-9');
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
