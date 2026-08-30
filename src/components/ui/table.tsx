import * as React from 'react';
import { Card } from '@/components/ui/card';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import {
  persistColumnWidths,
  readStoredColumnWidths,
} from './table-column-model';

/**
 * 全站表格视觉协议。
 *
 * - **default**：全站管理表默认（Dashboard 用量、Skills 三 Tab 等）— Card 壳 + 标准行线
 * - **workbench** / **flush**：无 Card 的贴边特例（目前业务侧不用；保留 API）
 *
 * 业务表只选 `TableShell variant`；表头/行/单元格密度由 Context 自动套用。
 */
export const tableStyles = {
  table: 'w-full border-collapse text-body',
  theadRow: 'border-b border-border bg-subtle text-left',
  th: 'px-3 py-2 text-meta font-medium text-muted',
  tr: 'border-t border-border/50 last:border-0 hover:bg-hover',
  trSelected: 'bg-hover/40',
  trActive: 'bg-active hover:bg-active',
  theadRowWorkbench: 'border-b border-border/70 bg-subtle/50 text-left',
  trWorkbench:
    'border-t border-border/40 last:border-0 hover:bg-hover data-[active=true]:bg-active data-[active=true]:hover:bg-active',
  thWorkbench: 'px-3 py-2 text-meta font-medium text-muted',
  tdWorkbench: 'px-3 py-2',
  td: 'px-3 py-2',
  footer:
    'flex flex-wrap items-center justify-between gap-2 border-t border-border px-3 py-2 text-meta text-muted',
  resizeHandle:
    'absolute right-0 top-0 z-10 h-full w-1.5 cursor-col-resize touch-none hover:bg-accent/40 active:bg-accent/60',
} as const;

export type TableShellVariant = 'default' | 'workbench' | 'flush';
export type TableDensity = 'default' | 'workbench';

export function createIdempotentCleanup<T extends unknown[]>(cleanup: (...args: T) => void) {
  let completed = false;
  return (...args: T) => {
    if (completed) return;
    completed = true;
    cleanup(...args);
  };
}

type TableShellContextValue = {
  variant: TableShellVariant;
  density: TableDensity;
};

const TableShellContext = React.createContext<TableShellContextValue>({
  variant: 'default',
  density: 'default',
});

function useTableShell(): TableShellContextValue {
  return React.useContext(TableShellContext);
}

function densityOf(variant: TableShellVariant): TableDensity {
  return variant === 'default' ? 'default' : 'workbench';
}

export type ColumnWidthSpec<K extends string = string> = {
  key: K;
  defaultWidth: number;
  minWidth: number;
};

/** 多列表头拖拽调宽（table-fixed + colgroup 消费 widths）；列宽按 storageKey 记住。 */
export function useColumnWidths<K extends string>(
  specs: ColumnWidthSpec<K>[],
  storageKey: string,
) {
  const defaults = React.useMemo(
    () =>
      Object.fromEntries(specs.map((s) => [s.key, s.defaultWidth])) as Record<K, number>,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [specs.map((s) => `${s.key}:${s.defaultWidth}:${s.minWidth}`).join('|')],
  );

  const minByKey = React.useMemo(
    () => Object.fromEntries(specs.map((s) => [s.key, s.minWidth])) as Record<K, number>,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [specs.map((s) => `${s.key}:${s.minWidth}`).join('|')],
  );

  const [widths, setWidths] = React.useState<Record<K, number>>(() =>
    readStoredColumnWidths(storageKey, defaults, minByKey),
  );
  const widthsRef = React.useRef(widths);
  widthsRef.current = widths;
  const dragRef = React.useRef<{
    key: K;
    startX: number;
    startWidth: number;
    minWidth: number;
    pointerId: number | null;
    cleanup: () => void;
  } | null>(null);

  const onResizeStart = React.useCallback(
    (key: K, e: React.MouseEvent | React.PointerEvent) => {
      if (dragRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      const isPointer = 'pointerId' in e;
      const pointerId = isPointer ? e.pointerId : null;
      const pointerTarget = e.currentTarget as HTMLElement;
      const previousCursor = document.body.style.cursor;
      const previousUserSelect = document.body.style.userSelect;

      dragRef.current = {
        key,
        startX: e.clientX,
        startWidth: widthsRef.current[key],
        minWidth: minByKey[key] ?? 48,
        pointerId,
        cleanup: () => undefined,
      };
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';

      const applyMove = (clientX: number) => {
        const drag = dragRef.current;
        if (!drag) return;
        const nextW = Math.max(drag.minWidth, drag.startWidth + (clientX - drag.startX));
        const prev = widthsRef.current;
        if (prev[drag.key] === nextW) return;
        const next = { ...prev, [drag.key]: nextW };
        widthsRef.current = next;
        setWidths(next);
      };

      const onPointerMove = (ev: PointerEvent) => {
        if (ev.pointerId !== pointerId) return;
        applyMove(ev.clientX);
      };
      const onMouseMove = (ev: MouseEvent) => applyMove(ev.clientX);
      let cleanup: () => void;
      const onPointerUp = (ev: PointerEvent) => {
        if (ev.pointerId !== pointerId) return;
        applyMove(ev.clientX);
        cleanup();
      };
      const onMouseUp = (ev: MouseEvent) => {
        applyMove(ev.clientX);
        cleanup();
      };
      const onPointerCancel = (ev: PointerEvent) => {
        if (ev.pointerId !== pointerId) return;
        cleanup();
      };
      cleanup = createIdempotentCleanup(() => {
        if (dragRef.current?.cleanup !== cleanup) return;
        dragRef.current = null;
        persistColumnWidths(storageKey, widthsRef.current);
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        window.removeEventListener('blur', cleanup);
        if (isPointer) {
          document.removeEventListener('pointermove', onPointerMove);
          document.removeEventListener('pointerup', onPointerUp);
          document.removeEventListener('pointercancel', onPointerCancel);
          try {
            pointerTarget.releasePointerCapture(pointerId!);
          } catch {
            // The pointer may already have been released by the browser.
          }
        } else {
          document.removeEventListener('mousemove', onMouseMove);
          document.removeEventListener('mouseup', onMouseUp);
        }
      });
      dragRef.current.cleanup = cleanup;

      window.addEventListener('blur', cleanup);
      if (isPointer) {
        document.addEventListener('pointermove', onPointerMove);
        document.addEventListener('pointerup', onPointerUp);
        document.addEventListener('pointercancel', onPointerCancel);
        try {
          pointerTarget.setPointerCapture(pointerId!);
        } catch {
          // Keep the document listeners as a compatibility fallback.
        }
      } else {
        document.addEventListener('mousemove', onMouseMove);
        document.addEventListener('mouseup', onMouseUp);
      }
    },
    [minByKey, storageKey],
  );

  React.useEffect(
    () => () => {
      dragRef.current?.cleanup();
    },
    [],
  );

  const totalWidth = React.useMemo(
    () => specs.reduce((sum, s) => sum + widths[s.key], 0),
    [specs, widths],
  );

  return { widths, onResizeStart, totalWidth, setWidths };
}

export function ColumnResizeHandle<K extends string>({
  columnKey,
  label,
  onResizeStart,
}: {
  columnKey: K;
  label: string;
  onResizeStart: (key: K, e: React.MouseEvent | React.PointerEvent) => void;
}) {
  const { t } = useI18n();
  return (
    <Hint label={t('common.resizeColumn')}>
      <span
        role="separator"
        aria-orientation="vertical"
        aria-label={t('common.resizeColumnNamed', { label })}
        onPointerDown={(e) => onResizeStart(columnKey, e)}
        onMouseDown={(e) => onResizeStart(columnKey, e)}
        className={tableStyles.resizeHandle}
      />
    </Hint>
  );
}

/**
 * 表格外壳。
 * - `default`：Card 边框 + 轻阴影（全站管理表默认，含 Skills）
 * - `workbench` / `flush`：无 Card 贴边（特例）
 */
export function TableShell({
  children,
  footer,
  className,
  variant = 'default',
}: {
  children: React.ReactNode;
  footer?: React.ReactNode;
  className?: string;
  variant?: TableShellVariant;
}) {
  const density = densityOf(variant);
  const ctx = React.useMemo(() => ({ variant, density }), [variant, density]);

  const body =
    variant === 'default' ? (
      <Card className={cn('overflow-hidden', className)} data-table-shell={variant}>
        <div className="overflow-x-auto">{children}</div>
        {footer}
      </Card>
    ) : (
      <div
        className={cn(
          'overflow-hidden rounded-none border-0 bg-transparent shadow-none',
          className,
        )}
        data-table-shell={variant}
      >
        <div className="overflow-x-auto">{children}</div>
        {footer}
      </div>
    );

  return <TableShellContext.Provider value={ctx}>{body}</TableShellContext.Provider>;
}

export const Table = React.forwardRef<
  HTMLTableElement,
  React.HTMLAttributes<HTMLTableElement>
>(({ className, ...props }, ref) => (
  <table ref={ref} className={cn(tableStyles.table, className)} {...props} />
));
Table.displayName = 'Table';

export const TableHeader = React.forwardRef<
  HTMLTableSectionElement,
  React.HTMLAttributes<HTMLTableSectionElement>
>(({ className, ...props }, ref) => (
  <thead ref={ref} className={cn(className)} {...props} />
));
TableHeader.displayName = 'TableHeader';

export const TableBody = React.forwardRef<
  HTMLTableSectionElement,
  React.HTMLAttributes<HTMLTableSectionElement>
>(({ className, ...props }, ref) => (
  <tbody ref={ref} className={cn(className)} {...props} />
));
TableBody.displayName = 'TableBody';

export const TableHeaderRow = React.forwardRef<
  HTMLTableRowElement,
  React.HTMLAttributes<HTMLTableRowElement> & {
    sticky?: boolean;
  }
>(({ className, sticky, ...props }, ref) => {
  const { density } = useTableShell();
  return (
    <tr
      ref={ref}
      className={cn(
        density === 'workbench' ? tableStyles.theadRowWorkbench : tableStyles.theadRow,
        sticky && 'sticky top-0 z-10 backdrop-blur-[2px]',
        className,
      )}
      {...props}
    />
  );
});
TableHeaderRow.displayName = 'TableHeaderRow';

export const TableRow = React.forwardRef<
  HTMLTableRowElement,
  React.HTMLAttributes<HTMLTableRowElement> & {
    selected?: boolean;
    active?: boolean;
  }
>(({ className, selected, active, ...props }, ref) => {
  const { density } = useTableShell();
  return (
    <tr
      ref={ref}
      data-active={active ? 'true' : undefined}
      className={cn(
        density === 'workbench' ? tableStyles.trWorkbench : tableStyles.tr,
        selected && tableStyles.trSelected,
        active && tableStyles.trActive,
        className,
      )}
      {...props}
    />
  );
});
TableRow.displayName = 'TableRow';

export const TableHead = React.forwardRef<
  HTMLTableCellElement,
  React.ThHTMLAttributes<HTMLTableCellElement>
>(({ className, ...props }, ref) => {
  const { density } = useTableShell();
  return (
    <th
      ref={ref}
      className={cn(
        density === 'workbench' ? tableStyles.thWorkbench : tableStyles.th,
        className,
      )}
      {...props}
    />
  );
});
TableHead.displayName = 'TableHead';

export const TableCell = React.forwardRef<
  HTMLTableCellElement,
  React.TdHTMLAttributes<HTMLTableCellElement>
>(({ className, ...props }, ref) => {
  const { density } = useTableShell();
  return (
    <td
      ref={ref}
      className={cn(
        density === 'workbench' ? tableStyles.tdWorkbench : tableStyles.td,
        className,
      )}
      {...props}
    />
  );
});
TableCell.displayName = 'TableCell';

export function TableFooterBar({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn(tableStyles.footer, className)} {...props} />;
}
