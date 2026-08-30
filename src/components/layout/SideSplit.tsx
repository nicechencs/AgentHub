import type { ReactNode } from 'react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';
import {
  SIDE_SPLIT_FRAME_PAD_RIGHT,
  SIDE_SPLIT_FRAME_PAD_Y,
} from './side-split-model';
import type { SideSplitController } from './use-side-split';

const separatorClass = cn(
  'group relative z-10 w-1.5 shrink-0 cursor-col-resize touch-none bg-transparent outline-none',
  'hover:bg-accent/40 focus-visible:bg-accent/40 active:bg-accent/60',
  'before:absolute before:inset-y-0 before:-left-1.5 before:-right-1.5 before:content-[""]',
);

export function SideSplitSeparator<T>({
  split,
  resizeAria,
}: {
  split: SideSplitController<T>;
  resizeAria: string;
}) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={resizeAria}
      aria-valuenow={split.paneWidth}
      aria-valuemin={split.valuemin}
      tabIndex={split.expanded ? 0 : -1}
      onPointerDown={split.expanded ? split.onResizeStart : undefined}
      onDoubleClick={split.expanded ? split.resetWidth : undefined}
      onKeyDown={split.expanded ? split.onSeparatorKeyDown : undefined}
      className={cn(separatorClass, !split.expanded && 'pointer-events-none opacity-0')}
    />
  );
}

export function SideSplitFrame<T>({
  split,
  resizeAria,
  padTop = 0,
  children,
}: {
  split: SideSplitController<T>;
  resizeAria: string;
  padTop?: number;
  children: ReactNode;
}) {
  if (!split.mounted) return null;
  return (
    <>
      <SideSplitSeparator split={split} resizeAria={resizeAria} />
      <div
        className={cn('h-full min-h-0 shrink-0 overflow-hidden', split.widthTransition)}
        style={{ width: split.shellWidth }}
        onTransitionEnd={split.onPaneTransitionEnd}
      >
        <div
          className="box-border flex h-full min-h-0"
          style={{
            width: split.paneWidth + SIDE_SPLIT_FRAME_PAD_RIGHT,
            paddingTop: padTop,
            paddingBottom: SIDE_SPLIT_FRAME_PAD_Y,
            paddingRight: SIDE_SPLIT_FRAME_PAD_RIGHT,
          }}
        >
          {children}
        </div>
      </div>
    </>
  );
}

/**
 * Full-height workbench: list column | optional inspect pane.
 * Page title lives in TopBar. Toolbar (tabs/filters + page commands) and
 * listFooter stay in the list column, left of the separator.
 * `flushTop` skips the shared 12px top inset when this split already sits
 * under another chrome row (Settings backups).
 */
export function WorkbenchSplitPage<T>({
  split,
  resizeAria,
  panel,
  listFooter,
  listOverflowX = 'auto',
  flushTop = false,
  children,
}: {
  split: SideSplitController<T>;
  resizeAria: string;
  panel?: ReactNode;
  /** Docked at the list column bottom-right, left of the separator. */
  listFooter?: ReactNode;
  /** Projects hides horizontal overflow while the preview is open. */
  listOverflowX?: 'auto' | 'hidden';
  /** Nested under an already-padded page chrome; both columns start flush. */
  flushTop?: boolean;
  children: ReactNode;
}) {
  const paneOpen = Boolean(split.mounted && panel);
  const listInset = paneOpen ? pageRhythm.workbenchXSplit : pageRhythm.workbenchX;
  const overflowX = paneOpen && listOverflowX === 'hidden' ? 'overflow-x-hidden' : 'overflow-x-auto';
  const padTop = flushTop ? 0 : SIDE_SPLIT_FRAME_PAD_Y;
  return (
    <div ref={split.splitRef} className="flex h-full min-h-0 overflow-hidden bg-canvas">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1 overflow-y-auto bg-canvas',
            overflowX,
            listInset,
            listFooter ? undefined : pageRhythm.workbenchY,
            padTop > 0 ? pageRhythm.workbenchPadT : undefined,
          )}
        >
          {children}
        </div>
        {listFooter ? (
          <div className={cn('flex shrink-0 justify-end pt-2', listInset, pageRhythm.workbenchY)}>
            {listFooter}
          </div>
        ) : null}
      </div>
      {split.mounted && panel ? (
        <SideSplitFrame split={split} resizeAria={resizeAria} padTop={padTop}>
          {panel}
        </SideSplitFrame>
      ) : null}
    </div>
  );
}
