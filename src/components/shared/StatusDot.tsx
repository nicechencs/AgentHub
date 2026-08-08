import type { AuthStatus } from '@/lib/types';
import { cn } from '@/lib/utils';

const COLOR: Record<AuthStatus, string> = {
  valid: 'bg-success',
  expiring: 'bg-warning',
  expired: 'bg-danger',
  none: 'bg-muted',
};

const LABEL: Record<AuthStatus, string> = {
  valid: '已认证',
  expiring: '即将过期',
  expired: '已失效',
  none: '未配置',
};

/** 四态认证状态点(有效/临期/失效/未配置) */
export function StatusDot({ status, withLabel = false }: { status: AuthStatus; withLabel?: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className={cn('inline-block h-2 w-2 rounded-full', COLOR[status])} />
      {withLabel && <span className="text-xs text-secondary">{LABEL[status]}</span>}
    </span>
  );
}
