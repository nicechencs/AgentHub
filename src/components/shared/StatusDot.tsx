import { useI18n } from '@/components/shared/LanguageProvider';
import type { MessageKey } from '@/lib/i18n';
import type { AuthStatus } from '@/lib/types';
import { cn } from '@/lib/utils';

const COLOR: Record<AuthStatus, string> = {
  valid: 'bg-success',
  expiring: 'bg-warning',
  expired: 'bg-danger',
  none: 'bg-muted',
};

const LABEL_KEY: Record<AuthStatus, MessageKey> = {
  valid: 'chrome.authStatus.valid',
  expiring: 'chrome.authStatus.expiring',
  expired: 'chrome.authStatus.expired',
  none: 'chrome.authStatus.none',
};

/** 四态认证状态点(有效/临期/失效/未配置) */
export function StatusDot({ status, withLabel = false }: { status: AuthStatus; withLabel?: boolean }) {
  const { t } = useI18n();
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className={cn('inline-block h-2 w-2 rounded-full', COLOR[status])} />
      {withLabel && <span className="text-meta text-secondary">{t(LABEL_KEY[status])}</span>}
    </span>
  );
}
