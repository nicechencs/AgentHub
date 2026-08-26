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
  children,
}: {
  split: SideSplitController<T>;
  resizeAria: string;
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
            paddingTop: 0,
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
 * Full-height workbench: list column (compact header + list) | optional inspect pane.
 * Header and optional listFooter stay in the list column so actions sit left
 * of the separator and travel with it while resizing — not the far edge of
 * the page.
 */
export function WorkbenchSplitPage<T>({
  header,
  split,
  resizeAria,
  panel,
  listFooter,
  listOverflowX = 'auto',
  children,
}: {
  header: ReactNode;
  split: SideSplitController<T>;
  resizeAria: string;
  panel?: ReactNode;
  /** Docked at the list column bottom-right, left of the separator. */
  listFooter?: ReactNode;
  /** Projects hides horizontal overflow while the preview is open. */
  listOverflowX?: 'auto' | 'hidden';
  children: ReactNode;
}) {
  const paneOpen = Boolean(split.mounted && panel);
  const listInset = paneOpen ? pageRhythm.workbenchXSplit : pageRhythm.workbenchX;
  const overflowX = paneOpen && listOverflowX === 'hidden' ? 'overflow-x-hidden' : 'overflow-x-auto';
  return (
    <div ref={split.splitRef} className="flex h-full min-h-0 overflow-hidden bg-canvas">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className={pageRhythm.workbenchHeader}>{header}</div>
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1 overflow-y-auto bg-canvas',
            overflowX,
            listInset,
            listFooter ? undefined : pageRhythm.workbenchY,
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
        <SideSplitFrame split={split} resizeAria={resizeAria}>
          {panel}
        </SideSplitFrame>
      ) : null}
    </div>
  );
}
