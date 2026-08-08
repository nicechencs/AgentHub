import * as React from 'react';
import * as ToastPrimitive from '@radix-ui/react-toast';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ToastData {
  id: string;
  title: string;
  description?: string;
  variant?: 'default' | 'success' | 'danger';
  /** 撤销等动作按钮 */
  actionLabel?: string;
  onAction?: () => void;
  /** 毫秒,默认 5000 */
  duration?: number;
}

interface ToastContextValue {
  toast: (t: Omit<ToastData, 'id'>) => void;
}

const ToastContext = React.createContext<ToastContextValue>({ toast: () => {} });

export function useToast() {
  return React.useContext(ToastContext);
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = React.useState<ToastData[]>([]);

  const toast = React.useCallback((t: Omit<ToastData, 'id'>) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    setToasts((prev) => [...prev, { ...t, id }]);
  }, []);

  const remove = (id: string) => setToasts((prev) => prev.filter((t) => t.id !== id));

  return (
    <ToastContext.Provider value={{ toast }}>
      <ToastPrimitive.Provider swipeDirection="right">
        {children}
        {toasts.map((t) => (
          <ToastPrimitive.Root
            key={t.id}
            duration={t.duration ?? 5000}
            onOpenChange={(open) => !open && remove(t.id)}
            className={cn(
              'pointer-events-auto flex items-center gap-3 rounded-card border border-border bg-panel px-4 py-3 shadow-md',
              t.variant === 'success' && 'border-success/40',
              t.variant === 'danger' && 'border-danger/40',
            )}
          >
            <div className="flex-1">
              <ToastPrimitive.Title className="text-sm font-medium">{t.title}</ToastPrimitive.Title>
              {t.description && (
                <ToastPrimitive.Description className="mt-0.5 text-xs text-secondary">
                  {t.description}
                </ToastPrimitive.Description>
              )}
            </div>
            {t.actionLabel && (
              <ToastPrimitive.Action
                altText={t.actionLabel}
                onClick={() => {
                  t.onAction?.();
                  remove(t.id);
                }}
                className="inline-flex h-6 items-center rounded-btn border border-accent/50 px-2.5 text-xs font-medium text-accent hover:bg-accent/10"
              >
                {t.actionLabel}
              </ToastPrimitive.Action>
            )}
            <ToastPrimitive.Close className="text-muted hover:text-primary">
              <X className="h-3.5 w-3.5" />
            </ToastPrimitive.Close>
          </ToastPrimitive.Root>
        ))}
        <ToastPrimitive.Viewport className="fixed bottom-4 right-4 z-[100] flex w-96 max-w-full flex-col gap-2 outline-none" />
      </ToastPrimitive.Provider>
    </ToastContext.Provider>
  );
}
