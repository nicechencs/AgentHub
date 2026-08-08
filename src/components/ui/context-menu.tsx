import * as React from 'react';
import { createPortal } from 'react-dom';
import { cn } from '@/lib/utils';

export type ContextMenuPoint = {
  x: number;
  y: number;
};

/** Lightweight fixed-position context menu (portal). */
export function ContextMenu({
  open,
  point,
  onClose,
  className,
  children,
}: {
  open: boolean;
  point: ContextMenuPoint | null;
  onClose: () => void;
  className?: string;
  children: React.ReactNode;
}) {
  const ref = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      if (ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    const onScrollOrResize = () => onClose();
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKey);
    window.addEventListener('scroll', onScrollOrResize, true);
    window.addEventListener('resize', onScrollOrResize);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('scroll', onScrollOrResize, true);
      window.removeEventListener('resize', onScrollOrResize);
    };
  }, [open, onClose]);

  // Keep the menu inside the viewport after paint.
  React.useLayoutEffect(() => {
    if (!open || !point || !ref.current) return;
    const el = ref.current;
    const rect = el.getBoundingClientRect();
    let x = point.x;
    let y = point.y;
    if (x + rect.width > window.innerWidth - 8) {
      x = Math.max(8, window.innerWidth - rect.width - 8);
    }
    if (y + rect.height > window.innerHeight - 8) {
      y = Math.max(8, window.innerHeight - rect.height - 8);
    }
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
  }, [open, point, children]);

  if (!open || !point || typeof document === 'undefined') return null;

  return createPortal(
    <div
      ref={ref}
      role="menu"
      className={cn(
        'fixed z-[60] min-w-[10rem] rounded-card border border-border bg-panel p-1 shadow-md',
        className,
      )}
      style={{ left: point.x, top: point.y }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {children}
    </div>,
    document.body,
  );
}

export function ContextMenuItem({
  className,
  onSelect,
  disabled,
  children,
  ...props
}: Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, 'onClick'> & {
  onSelect?: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      className={cn(
        'flex w-full cursor-default select-none items-center gap-2 rounded-btn px-2 py-1.5 text-left text-sm outline-none',
        'hover:bg-hover focus:bg-hover disabled:pointer-events-none disabled:opacity-50',
        className,
      )}
      onClick={(e) => {
        e.stopPropagation();
        if (disabled) return;
        onSelect?.();
      }}
      {...props}
    >
      {children}
    </button>
  );
}
