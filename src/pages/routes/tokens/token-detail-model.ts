/**
 * Pure view-model for the local-token detail pane. No React, no IO.
 */
import { ROUTE_ENDPOINT_HOST, routeEndpointHttpParts } from '@/lib/route-endpoints';
import type { TranslateFn } from '@/lib/i18n';
import { fmtTokens } from '@/lib/utils';
import {
  tokenListenPort,
  tokenTypeLabel,
  type LocalTokenRow,
  type LocalTokenUsage,
} from './tokens-model';

export type TokenDetailCopyRow = {
  id: 'type' | 'endpoint' | 'token';
  label: string;
  display: string;
  copyValue: string | null;
  pending: boolean;
};

export function tokenEndpointParts(row: Pick<LocalTokenRow, 'path' | 'endpoint' | 'kind'>) {
  return routeEndpointHttpParts({
    path: row.path,
    port: tokenListenPort(row.endpoint),
    host: ROUTE_ENDPOINT_HOST,
    endpointId: row.kind === 'chat_completions'
      ? 'chat_completions'
      : row.kind === 'messages'
        ? 'messages'
        : 'responses',
  });
}

export function buildTokenDetailCopyRows(
  row: LocalTokenRow,
  revealed: boolean,
  t?: TranslateFn,
): TokenDetailCopyRow[] {
  const endpoint = tokenEndpointParts(row);
  const tokenDisplay = row.unavailable
    ? (t ? t('routes.runtime.unavailable') : '状态不可用')
    : revealed
      ? (row.token ?? '')
      : (row.maskedToken ?? '');
  const typeLabel = tokenTypeLabel(row, t);
  return [
    {
      id: 'type',
      label: t ? t('routes.tokens.fieldType') : '类型',
      display: typeLabel,
      copyValue: null,
      pending: false,
    },
    {
      id: 'endpoint',
      label: t ? t('routes.tokens.fieldEndpoint') : '端点',
      display: endpoint.display,
      copyValue: endpoint.href ?? endpoint.display,
      pending: endpoint.portPending,
    },
    {
      id: 'token',
      label: t ? t('routes.tokens.fieldToken') : '入口 Key',
      display: tokenDisplay,
      copyValue: row.unavailable ? null : row.token,
      pending: !row.token && !row.unavailable,
    },
  ];
}

export function tokenDetailTitle(row: LocalTokenRow, t?: TranslateFn): string {
  return tokenTypeLabel(row, t);
}

export function formatTokenRelative(iso: string | null | undefined, t?: TranslateFn): string {
  if (!iso) return '';
  const parsed = Date.parse(iso);
  if (Number.isNaN(parsed)) return '';
  const diff = Date.now() - parsed;
  const m = Math.floor(diff / 60000);
  if (m < 1) return t ? t('common.relativeJustNow') : '刚刚';
  if (m < 60) return t ? t('common.relativeMinutes', { n: m }) : `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return t ? t('common.relativeHours', { n: h }) : `${h} 小时前`;
  const d = Math.floor(h / 24);
  return t ? t('common.relativeDays', { n: d }) : `${d} 天前`;
}

export function tokenLastPageDisplay(row: Pick<LocalTokenRow, 'lastPath'>): string {
  return row.lastPath?.trim() || '';
}

export function tokenUsageDisplay(
  usage: LocalTokenUsage | undefined,
  t?: TranslateFn,
): string {
  if (!usage || usage.requestCount <= 0) return '';
  const input = fmtTokens(usage.inputTokens);
  const output = fmtTokens(usage.outputTokens);
  return t
    ? t('routes.tokens.usageSummary', { in: input, out: output })
    : `${input} in / ${output} out`;
}
