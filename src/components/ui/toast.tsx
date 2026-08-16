import * as React from 'react';
import * as ToastPrimitive from '@radix-ui/react-toast';
import { Check, Copy, X } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ToastAutoDismissController } from './toast-auto-dismiss';

/** 默认展示时长（毫秒） */
export const DEFAULT_TOAST_DURATION_MS = 5_000;

export interface ToastData {
  id: string;
  title: string;
  description?: string;
  variant?: 'default' | 'success' | 'danger';
  /** 撤销等动作按钮 */
  actionLabel?: string;
  onAction?: () => void;
  /** 毫秒；默认 {@link DEFAULT_TOAST_DURATION_MS}；`Infinity` 表示不自动关闭 */
  duration?: number;
}

interface ToastContextValue {
  toast: (t: Omit<ToastData, 'id'>) => void;
}

const ToastContext = React.createContext<ToastContextValue>({ toast: () => {} });

export function useToast() {
  return React.useContext(ToastContext);
}

/** 拼出可复制的 toast 纯文本（标题 + 描述） */
export function formatToastClipboardText(title: string, description?: string): string {
  const t = title.trim();
  const d = description?.trim();
  if (!d) return t;
  return `${t}\n${d}`;
}

/**
 * 自管自动关闭：仅 hover 暂停。避免 Radix pause-on-focus / window-blur 卡住。
 */
function useToastAutoDismiss(
  enabled: boolean,
  durationMs: number,
  onDismiss: () => void,
): {
  onMouseEnter: () => void;
  onMouseLeave: () => void;
} {
  const onDismissRef = React.useRef(onDismiss);
  onDismissRef.current = onDismiss;
  const controllerRef = React.useRef<ToastAutoDismissController | null>(null);

  React.useEffect(() => {
    controllerRef.current?.dispose();
    controllerRef.current = null;
    if (!enabled || !Number.isFinite(durationMs) || durationMs <= 0) {
      return;
    }
    const controller = new ToastAutoDismissController({
      durationMs,
      onDismiss: () => onDismissRef.current(),
    });
    controllerRef.current = controller;
    controller.start();
    return () => {
      controller.dispose();
      if (controllerRef.current === controller) controllerRef.current = null;
    };
  }, [durationMs, enabled]);

  return {
    onMouseEnter: () => controllerRef.current?.pause(),
    onMouseLeave: () => controllerRef.current?.resume(),
  };
}

function ToastItem({
  t,
  onRemove,
}: {
  t: ToastData;
  onRemove: (id: string) => void;
}) {
  const [open, setOpen] = React.useState(true);
  const [copied, setCopied] = React.useState(false);
  const copiedTimer = React.useRef<number | null>(null);

  const durationMs = t.duration ?? DEFAULT_TOAST_DURATION_MS;
  const autoDismiss = Number.isFinite(durationMs) && durationMs > 0;

  // 受控 open 时父级 setOpen(false) 不会走 Radix onOpenChange，须同步 onRemove。
  const requestClose = React.useCallback(() => {
    setOpen(false);
    onRemove(t.id);
  }, [onRemove, t.id]);

  const hover = useToastAutoDismiss(open && autoDismiss, durationMs, requestClose);

  React.useEffect(() => {
    return () => {
      if (copiedTimer.current != null) window.clearTimeout(copiedTimer.current);
    };
  }, []);

  const copyText = React.useCallback(async () => {
    const text = formatToastClipboardText(t.title, t.description);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // fallback for restricted webviews
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.left = '-9999px';
      document.body.appendChild(ta);
      ta.select();
      try {
        document.execCommand('copy');
      } finally {
        document.body.removeChild(ta);
      }
    }
    setCopied(true);
    if (copiedTimer.current != null) window.clearTimeout(copiedTimer.current);
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1500);
  }, [t.description, t.title]);

  return (
    <ToastPrimitive.Root
      open={open}
      // 关闭 Radix 自带 duration（其 pause-on-focus / window-blur 在桌面端会卡住）
      duration={Infinity}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) onRemove(t.id);
      }}
      onMouseEnter={hover.onMouseEnter}
      onMouseLeave={hover.onMouseLeave}
      // Radix 默认 userSelect:none + 滑动手势会抢走拖选；覆盖为可选中文本
      style={{ userSelect: 'text', touchAction: 'pan-y' }}
      className={cn(
        'pointer-events-auto flex items-start gap-3 rounded-card border border-border bg-panel px-4 py-3 shadow-md',
        t.variant === 'success' && 'border-success/40',
        t.variant === 'danger' && 'border-danger/40',
      )}
    >
      {/* stopPropagation：避免 Root 的 swipe pointer 跟踪打断文字选中 */}
      <div
        className="min-w-0 flex-1 select-text"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <ToastPrimitive.Title className="select-text text-body font-medium">
          {t.title}
        </ToastPrimitive.Title>
        {t.description && (
          <ToastPrimitive.Description className="mt-0.5 select-text break-words text-meta text-secondary">
            {t.description}
          </ToastPrimitive.Description>
        )}
      </div>
      {t.actionLabel && (
        <ToastPrimitive.Action
          altText={t.actionLabel}
          onClick={() => {
            t.onAction?.();
            requestClose();
          }}
          className="inline-flex h-6 shrink-0 items-center rounded-btn border border-accent/50 px-2.5 text-meta font-medium text-accent hover:bg-accent/10"
        >
          {t.actionLabel}
        </ToastPrimitive.Action>
      )}
      <button
        type="button"
        aria-label={copied ? '已复制' : '复制消息'}
        title={copied ? '已复制' : '复制消息'}
        onClick={() => void copyText()}
        onPointerDown={(e) => e.stopPropagation()}
        className="mt-0.5 shrink-0 text-muted hover:text-primary"
      >
        {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
      </button>
      <ToastPrimitive.Close
        className="mt-0.5 shrink-0 text-muted hover:text-primary"
        aria-label="关闭"
      >
        <X className="h-3.5 w-3.5" />
      </ToastPrimitive.Close>
    </ToastPrimitive.Root>
  );
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = React.useState<ToastData[]>([]);

  const toast = React.useCallback((t: Omit<ToastData, 'id'>) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    setToasts((prev) => [...prev, { ...t, id }]);
  }, []);

  const remove = React.useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toast }}>
      <ToastPrimitive.Provider swipeDirection="right" duration={DEFAULT_TOAST_DURATION_MS}>
        {children}
        {toasts.map((t) => (
          <ToastItem key={t.id} t={t} onRemove={remove} />
        ))}
        <ToastPrimitive.Viewport className="fixed bottom-4 right-4 z-[100] flex w-96 max-w-full flex-col gap-2 outline-none" />
      </ToastPrimitive.Provider>
    </ToastContext.Provider>
  );
}
