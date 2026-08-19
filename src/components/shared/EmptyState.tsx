import type { ReactNode } from 'react';
import type { LucideIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';

/** 空状态:图标 + 一句话 + 主行动按钮(docs/ui-design.md §6) */
export function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  action,
}: {
  icon: LucideIcon;
  title: string;
  description?: string;
  actionLabel?: string;
  onAction?: () => void;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-card border border-dashed border-border py-10 text-center">
      <Icon className="h-6 w-6 text-muted" />
      <p className="text-title font-medium text-primary">{title}</p>
      {description && <p className="max-w-sm text-meta text-muted">{description}</p>}
      {action ?? (actionLabel && onAction ? (
        <Button size="sm" className="mt-2" onClick={onAction}>
          {actionLabel}
        </Button>
      ) : null)}
    </div>
  );
}
