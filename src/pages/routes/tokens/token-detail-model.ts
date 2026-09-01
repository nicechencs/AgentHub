/**
 * Pure view-model for the local-token detail pane. No React, no IO.
 */
import { ROUTE_ENDPOINT_HOST, routeEndpointHttpParts } from '@/lib/route-endpoints';
import type { TranslateFn } from '@/lib/i18n';
import { fmtTokens } from '@/lib/utils';
import type { LocalTokenProbeOutcome, LocalTokenProbeResult } from '@/lib/backend/contracts/adapter';
import {
  maskLocalToken,
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

export type LocalTokenTestGate = {
  enabled: boolean;
  reason: string | null;
};

export function localTokenTestGate(
  row: Pick<LocalTokenRow, 'token' | 'endpoint' | 'unavailable'>,
  t?: TranslateFn,
): LocalTokenTestGate {
  if (row.unavailable) {
    return {
      enabled: false,
      reason: t ? t('routes.runtime.unavailable') : '状态不可用',
    };
  }
  if (!row.token?.trim()) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.testNeedKey') : '先填写入口 Key',
    };
  }
  if (tokenListenPort(row.endpoint) == null) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.testNeedEndpoint') : '本机入口还没启动',
    };
  }
  return { enabled: true, reason: null };
}

export function localTokenTestResultLabel(
  result: Pick<LocalTokenProbeResult, 'outcome' | 'latencyMs'>,
  t?: TranslateFn,
): string {
  const outcome: LocalTokenProbeOutcome = result.outcome;
  if (outcome === 'ok') {
    const ms = Math.max(0, Math.round(result.latencyMs));
    return t
      ? t('routes.tokens.testOkMs', { ms: String(ms) })
      : `入口 Key 可用 · ${ms}ms`;
  }
  if (outcome === 'unauthorized') {
    return t ? t('routes.tokens.testUnauthorized') : '入口 Key 无效';
  }
  if (outcome === 'unreachable') {
    return t ? t('routes.tokens.testUnreachable') : '端点连不上';
  }
  if (outcome === 'rejected') {
    return t ? t('routes.tokens.testRejected') : '本机入口暂时不可用';
  }
  return t ? t('routes.tokens.testInvalid') : '没法测试';
}

export function localTokenTestResultTone(
  outcome: LocalTokenProbeOutcome,
): 'success' | 'danger' | 'muted' {
  if (outcome === 'ok') return 'success';
  if (outcome === 'invalid') return 'muted';
  return 'danger';
}

export function localTokenEntryRunning(
  row: Pick<LocalTokenRow, 'state'>,
): boolean {
  return row.state === 'running' || row.state === 'degraded';
}

export function localTokenTestRequestUrl(
  row: Pick<LocalTokenRow, 'endpoint'>,
  result?: Pick<LocalTokenProbeResult, 'requestUrl'> | null,
): string {
  if (result?.requestUrl?.trim()) return result.requestUrl.trim();
  const port = tokenListenPort(row.endpoint);
  return port ? `http://127.0.0.1:${port}/health` : '';
}

export function localTokenTestInputText(
  row: Pick<LocalTokenRow, 'token' | 'maskedToken' | 'endpoint'>,
  result?: Pick<LocalTokenProbeResult, 'requestUrl'> | null,
): string {
  const url = localTokenTestRequestUrl(row, result) || '—';
  const key = row.maskedToken?.trim()
    || (row.token ? maskLocalToken(row.token) : '—');
  return `GET ${url}\nAuthorization: Bearer ${key}`;
}

export function localTokenTestOutputText(
  result: LocalTokenProbeResult | null,
  options: { running: boolean; testing: boolean },
  t?: TranslateFn,
): string {
  if (options.testing) {
    return t ? t('routes.tokens.testing') : '测试中…';
  }
  if (!result) {
    return t ? t('routes.tokens.testNoOutput') : '没有返回';
  }
  const lines: string[] = [];
  if (!options.running) {
    lines.push(t ? t('routes.tokens.testNeedEndpoint') : '本机入口还没启动');
  }
  if (result.httpStatus != null) {
    lines.push(`HTTP ${result.httpStatus}`);
  }
  if (result.errorMessage?.trim()) {
    lines.push(result.errorMessage.trim());
  }
  if (result.responseBody?.trim()) {
    lines.push(result.responseBody.trim());
  }
  if (lines.length === 0) {
    return t ? t('routes.tokens.testNoOutput') : '没有返回';
  }
  return lines.join('\n');
}

export function localTokenTestDurationLabel(
  latencyMs: number | null | undefined,
  t?: TranslateFn,
): string {
  if (latencyMs == null) return '';
  const ms = Math.max(0, Math.round(latencyMs));
  return t ? t('routes.tokens.testDurationMs', { ms: String(ms) }) : `${ms}ms`;
}
