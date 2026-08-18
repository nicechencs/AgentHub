import type { TranslateFn } from '@/lib/i18n';
import type { AppSettings } from '@/lib/types';

export function fmtRelativeI18n(iso: string | undefined, t: TranslateFn): string {
  if (!iso) return '—';
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return t('common.relativeJustNow');
  if (m < 60) return t('common.relativeMinutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('common.relativeHours', { n: h });
  return t('common.relativeDays', { n: Math.floor(h / 24) });
}

export function fmtAbsoluteI18n(iso: string, lang: AppSettings['language']): string {
  return new Date(iso).toLocaleString(lang === 'en' ? 'en-US' : 'zh-CN', { hour12: false });
}
