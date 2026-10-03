import type { ImportLoginReport } from '@/lib/api/account';
import type { TranslateFn } from '@/lib/i18n';

export interface ImportLoginNotice {
  title: string;
  description?: string;
  variant: 'success' | 'warning' | 'danger';
}

/**
 * Toast for a manual「导入本机登录」. `baseDescription` is the usual
 * "{label} 已加入列表" line; restored, skipped and failed entries follow it.
 */
export function importLoginReportNotice(
  report: ImportLoginReport,
  baseDescription: string,
  t: TranslateFn,
): ImportLoginNotice {
  const parts = [baseDescription];
  if (report.restoredFromTrash.length > 0) {
    parts.push(t('connections.import.toastRestoredFromTrash', {
      labels: report.restoredFromTrash.map((item) => item.label).join('、'),
    }));
  }
  if (report.failed.length > 0) {
    parts.push(t('connections.import.toastSomeFailed', {
      n: report.failed.length,
      labels: report.failed.map((item) => item.label).join('、'),
    }));
  }
  if (report.skippedLocalRoute > 0) {
    parts.push(t('connections.import.toastSkippedLocalRoute', { n: report.skippedLocalRoute }));
  }
  return {
    title: t('connections.import.toastOk'),
    description: parts.join(' '),
    variant: report.failed.length === 0 ? 'success' : 'warning',
  };
}

/** Toast for a failed manual import. */
export function importLoginErrorNotice(error: unknown, t: TranslateFn): ImportLoginNotice {
  return {
    title: t('connections.import.toastFail'),
    description: error instanceof Error ? error.message : String(error),
    variant: 'danger',
  };
}
