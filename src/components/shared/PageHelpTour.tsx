import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  pageHelpCopy,
  PAGE_HELP_STEP_COUNT,
  pageHelpStepSelector,
  type PageHelpId,
} from '@/components/shared/page-help-model';
import {
  dimPaneRects,
  filterVisibleHelpSteps,
  HELP_BUBBLE_WIDTH,
  helpViewportSize,
  pageHelpKeyAction,
  pickHelpTargetRect,
  placeHelpBubble,
  resolveHelpTarget,
  visibleOverlap,
  type HelpBubbleLayout,
  type HelpDock,
} from '@/components/shared/page-help-tour';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

function arrowClass(placement: HelpBubbleLayout['placement']): string {
  const base = 'pointer-events-none absolute h-2 w-2 rotate-45 border-border bg-panel';
  if (placement === 'bottom') return cn(base, 'top-0 -translate-x-1/2 -translate-y-1/2 border-l border-t');
  if (placement === 'top') return cn(base, 'bottom-0 -translate-x-1/2 translate-y-1/2 border-b border-r');
  if (placement === 'right') return cn(base, 'left-0 -translate-x-1/2 -translate-y-1/2 border-b border-l');
  return cn(base, 'right-0 translate-x-1/2 -translate-y-1/2 border-r border-t');
}

function arrowStyle(layout: HelpBubbleLayout): { left?: number; top?: number } {
  if (layout.placement === 'top' || layout.placement === 'bottom') {
    return { left: layout.arrowOffset };
  }
  return { top: layout.arrowOffset };
}

function HelpKey({
  children,
  onAccent = false,
}: {
  children: string;
  onAccent?: boolean;
}) {
  return (
    <kbd
      className={cn(
        'inline-flex h-5 min-w-5 items-center justify-center rounded-btn border px-1 text-meta leading-none',
        onAccent ? 'border-white/35 bg-white/15 text-white' : 'border-border bg-subtle text-muted',
      )}
      aria-hidden
    >
      {children}
    </kbd>
  );
}

function sameRect(
  a: HelpBubbleLayout['highlight'],
  b: HelpBubbleLayout['highlight'],
): boolean {
  if (a == null || b == null) return a === b;
  return a.top === b.top && a.left === b.left && a.width === b.width && a.height === b.height;
}

function sameLayout(prev: HelpBubbleLayout | null, next: HelpBubbleLayout): boolean {
  return Boolean(
    prev &&
      prev.top === next.top &&
      prev.left === next.left &&
      prev.placement === next.placement &&
      prev.arrowOffset === next.arrowOffset &&
      prev.dock === next.dock &&
      sameRect(prev.highlight, next.highlight),
  );
}

function isTourNode(node: Node): boolean {
  const el = node instanceof Element ? node : node.parentElement;
  return Boolean(el?.closest('[data-page-help-tour]'));
}

/** Click-to-start page tour: spotlight + bubble on one control at a time. */
export function PageHelpTour({
  open,
  helpId,
  onClose,
}: {
  open: boolean;
  helpId: PageHelpId;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const copy = pageHelpCopy(helpId);
  const [cursor, setCursor] = useState(0);
  const [visible, setVisible] = useState<number[]>(() =>
    Array.from({ length: PAGE_HELP_STEP_COUNT[helpId] }, (_, i) => i),
  );
  const bubbleRef = useRef<HTMLDivElement>(null);
  const dockRef = useRef<{ helpId: PageHelpId; dock: HelpDock } | null>(null);
  const remeasureRef = useRef<(() => void) | null>(null);
  const [layout, setLayout] = useState<HelpBubbleLayout | null>(null);
  const step = visible[Math.min(cursor, Math.max(0, visible.length - 1))] ?? 0;

  useEffect(() => {
    setCursor(0);
    if (!open || typeof document === 'undefined') {
      setVisible(Array.from({ length: PAGE_HELP_STEP_COUNT[helpId] }, (_, i) => i));
      return;
    }
    const read = () => {
      const found = Array.from({ length: PAGE_HELP_STEP_COUNT[helpId] }, (_, i) =>
        Boolean(resolveHelpTarget(pageHelpStepSelector(helpId, i))),
      );
      const next = filterVisibleHelpSteps(found);
      setVisible((prev) => (prev.join() === next.join() ? prev : next));
    };
    read();
    const frame = window.requestAnimationFrame(read);
    const observer = new MutationObserver((records) => {
      if (records.every((record) => isTourNode(record.target))) return;
      window.requestAnimationFrame(() => {
        read();
        remeasureRef.current?.();
      });
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [helpId, open]);

  useLayoutEffect(() => {
    if (!open) {
      dockRef.current = null;
      setLayout(null);
      return;
    }
    const preferred = pageHelpStepSelector(helpId, step);
    const apply = () => {
      const viewport = helpViewportSize();
      const picked = pickHelpTargetRect(preferred, document, false, viewport);
      const measured = picked?.rect ?? null;
      const target = measured ? (visibleOverlap(measured, viewport) ?? measured) : null;
      const bubbleH = bubbleRef.current?.offsetHeight ?? 160;
      const next = placeHelpBubble({
        target,
        viewport,
        bubble: { width: HELP_BUBBLE_WIDTH, height: bubbleH },
        previousDock: dockRef.current?.helpId === helpId ? dockRef.current.dock : null,
      });
      dockRef.current = next.dock === 'adjacent' ? null : { helpId, dock: next.dock };
      setLayout((prev) => (sameLayout(prev, next) ? prev : next));
      return picked?.element ?? null;
    };
    const viewport = helpViewportSize();
    const first = pickHelpTargetRect(preferred, document, false, viewport);
    if (first?.element && !visibleOverlap(first.rect, viewport)) {
      first.element.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    }
    const targetEl = apply();
    remeasureRef.current = apply;
    const frame = window.requestAnimationFrame(apply);
    window.addEventListener('resize', apply);
    window.addEventListener('scroll', apply, true);
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(apply);
    if (targetEl) observer?.observe(targetEl);
    return () => {
      remeasureRef.current = null;
      window.cancelAnimationFrame(frame);
      window.removeEventListener('resize', apply);
      window.removeEventListener('scroll', apply, true);
      observer?.disconnect();
    };
  }, [cursor, helpId, open, step, visible.length]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      const action = pageHelpKeyAction(event, event.target);
      if (!action) return;
      event.preventDefault();
      if (action === 'skip') {
        onClose();
        return;
      }
      if (action === 'back') {
        setCursor((n) => Math.max(0, n - 1));
        return;
      }
      if (cursor >= visible.length - 1) onClose();
      else setCursor((n) => n + 1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [cursor, open, onClose, visible.length]);

  if (!open || typeof document === 'undefined') return null;

  const last = cursor >= visible.length - 1;
  const body = copy.steps[step];

  const dimPanes = dimPaneRects(
    layout?.highlight ?? null,
    typeof window === 'undefined'
      ? { width: 0, height: 0 }
      : { width: window.innerWidth, height: window.innerHeight },
  );

  return createPortal(
    <div
      className="pointer-events-none fixed inset-0 z-50"
      data-page-help-tour={helpId}
      data-page-help-step={step}
    >
      {dimPanes.map((pane, i) => (
        <button
          key={i}
          type="button"
          className="pointer-events-auto absolute cursor-default bg-black/45"
          style={{
            top: pane.top,
            left: pane.left,
            width: pane.width,
            height: pane.height,
          }}
          aria-label={t('chrome.pageHelp.skip')}
          onClick={onClose}
        />
      ))}
      {layout?.highlight ? (
        <div
          className="pointer-events-none absolute rounded-card ring-2 ring-accent"
          style={{
            top: layout.highlight.top,
            left: layout.highlight.left,
            width: layout.highlight.width,
            height: layout.highlight.height,
          }}
        />
      ) : null}
      <div
        ref={bubbleRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="page-help-tour-title"
        className="pointer-events-auto absolute rounded-card border border-border bg-panel px-4 py-3.5 shadow-md"
        data-page-help-dock={layout?.dock ?? undefined}
        style={{
          top: layout?.top ?? 48,
          left: layout?.left ?? 48,
          width: HELP_BUBBLE_WIDTH,
          visibility: layout ? 'visible' : 'hidden',
        }}
        onClick={(event) => event.stopPropagation()}
      >
        {layout ? (
          <span className={arrowClass(layout.placement)} style={arrowStyle(layout)} aria-hidden />
        ) : null}
        <p id="page-help-tour-title" className="text-title text-primary">
          {body ? t(body) : t(copy.intro)}
        </p>
        <div className="mt-3 flex flex-col gap-2">
          <span className="text-body text-muted">
            {t('chrome.pageHelp.progress', {
              current: cursor + 1,
              total: visible.length,
            })}
          </span>
          <div className="flex flex-nowrap items-center justify-end gap-1.5">
            <Button
              variant="ghost"
              className="shrink-0 gap-1.5"
              aria-keyshortcuts="Escape"
              onClick={onClose}
            >
              {t('chrome.pageHelp.skip')}
              <HelpKey>Esc</HelpKey>
            </Button>
            <Button
              variant="ghost"
              className="shrink-0 gap-1.5"
              disabled={cursor === 0}
              aria-keyshortcuts="ArrowLeft"
              onClick={() => setCursor((n) => n - 1)}
            >
              {t('chrome.pageHelp.back')}
              <HelpKey>←</HelpKey>
            </Button>
            <Button
              className="shrink-0 gap-1.5"
              aria-keyshortcuts={last ? 'Enter' : 'ArrowRight'}
              onClick={() => {
                if (last) onClose();
                else setCursor((n) => n + 1);
              }}
            >
              {last ? t('common.done') : t('chrome.pageHelp.next')}
              <HelpKey onAccent>{last ? 'Enter' : '→'}</HelpKey>
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
