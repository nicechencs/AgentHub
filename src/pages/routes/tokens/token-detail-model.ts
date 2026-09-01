/**
 * Pure view-model for the local-token detail pane. No React, no IO.
 */
import { ROUTE_ENDPOINT_HOST, routeEndpointHttpParts } from '@/lib/route-endpoints';
import type { TranslateFn } from '@/lib/i18n';
import {
  tokenListenPort,
  tokenTypeLabel,
  type LocalTokenRow,
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
      ? (row.token ?? (t ? t('routes.tokens.noToken') : '启动本机入口后才会生成 Key'))
      : (row.maskedToken ?? (t ? t('routes.tokens.noToken') : '启动本机入口后才会生成 Key'));
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
