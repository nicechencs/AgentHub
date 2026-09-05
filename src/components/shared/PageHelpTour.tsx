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
  pickHelpTargetRect,
  placeHelpBubble,
  resolveHelpTarget,
  type HelpBubbleLayout,
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
      setVisible(filterVisibleHelpSteps(found));
    };
    read();
    const frame = window.requestAnimationFrame(read);
    const observer = new MutationObserver(() => {
      window.requestAnimationFrame(read);
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [helpId, open]);

  useLayoutEffect(() => {
    if (!open) {
      setLayout(null);
      return;
    }
    const update = () => {
      const preferred = pageHelpStepSelector(helpId, step);
      const preferredMissing = !resolveHelpTarget(preferred);
      const picked = pickHelpTargetRect(preferred, document, preferredMissing);
      if (picked?.selector) {
        const node = document.querySelector(picked.selector);
        if (node instanceof HTMLElement) {
          node.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        }
      }
      const target = pickHelpTargetRect(preferred, document, true)?.rect ?? null;
      const bubbleH = bubbleRef.current?.offsetHeight ?? 120;
      setLayout(
        placeHelpBubble({
          target,
          viewport: { width: window.innerWidth, height: window.innerHeight },
          bubble: { width: HELP_BUBBLE_WIDTH, height: bubbleH },
        }),
      );
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }, [cursor, helpId, open, step, visible.length]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

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
        className="pointer-events-auto absolute rounded-card border border-border bg-panel px-3 py-2.5 shadow-md"
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
        <p id="page-help-tour-title" className="text-body text-primary">
          {body ? t(body) : t(copy.intro)}
        </p>
        <div className="mt-2.5 flex items-center gap-2">
          <span className="mr-auto text-meta text-muted">
            {t('chrome.pageHelp.progress', {
              current: cursor + 1,
              total: visible.length,
            })}
          </span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t('chrome.pageHelp.skip')}
          </Button>
          {cursor > 0 ? (
            <Button size="sm" variant="ghost" onClick={() => setCursor((n) => n - 1)}>
              {t('chrome.pageHelp.back')}
            </Button>
          ) : null}
          <Button
            size="sm"
            onClick={() => {
              if (last) onClose();
              else setCursor((n) => n + 1);
            }}
          >
            {last ? t('common.done') : t('chrome.pageHelp.next')}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
