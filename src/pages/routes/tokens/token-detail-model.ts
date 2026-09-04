/**
 * Pure view-model for the local-token detail pane. No React, no IO.
 */
import { ROUTE_ENDPOINT_HOST, routeEndpointHttpParts } from '@/lib/route-endpoints';
import type { TranslateFn } from '@/lib/i18n';
import type { LocalTokenProbeOutcome, LocalTokenProbeResult } from '@/lib/backend/contracts/adapter';
import {
  maskLocalToken,
  tokenDisplayName,
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
  return tokenDisplayName(row, t);
}

export type LocalTokenTestGate = {
  enabled: boolean;
  reason: string | null;
};

export function localTokenTestModels(
  row: Pick<LocalTokenRow, 'listedModels'>,
): string[] {
  return row.listedModels.filter((model) => model.trim().length > 0);
}

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
      reason: t ? t('routes.tokens.testNeedEndpoint') : '本机转发还没启动',
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
      : `连上模型了 · ${ms}ms`;
  }
  if (outcome === 'unauthorized') {
    return t ? t('routes.tokens.testUnauthorized') : '入口 Key 无效';
  }
  if (outcome === 'unreachable') {
    return t ? t('routes.tokens.testUnreachable') : '端点连不上';
  }
  if (outcome === 'rejected') {
    return t ? t('routes.tokens.testRejected') : '模型没通';
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
  row: Pick<LocalTokenRow, 'endpoint' | 'path'>,
  result?: Pick<LocalTokenProbeResult, 'requestUrl'> | null,
): string {
  if (result?.requestUrl?.trim()) return result.requestUrl.trim();
  const port = tokenListenPort(row.endpoint);
  const path = row.path?.startsWith('/') ? row.path : '/v1/chat/completions';
  return port ? `http://127.0.0.1:${port}${path}` : '';
}

export function localTokenTestInputText(
  row: Pick<LocalTokenRow, 'token' | 'maskedToken' | 'endpoint' | 'path'>,
  result?: Pick<LocalTokenProbeResult, 'requestUrl' | 'requestMethod' | 'requestBody'> | null,
): string {
  const url = localTokenTestRequestUrl(row, result) || '—';
  const method = result?.requestMethod?.trim() || 'POST';
  const key = row.maskedToken?.trim()
    || (row.token ? maskLocalToken(row.token) : '—');
  const header = `${method} ${url}\nAuthorization: Bearer ${key}`;
  const body = result?.requestBody?.trim();
  return body ? `${header}\n\n${body}` : header;
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
  if (!options.running && result.outcome === 'unreachable') {
    lines.push(t ? t('routes.tokens.testNeedEndpoint') : '本机转发还没启动');
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

export function localTokenTestWindowSummary(
  result: Pick<LocalTokenProbeResult, 'outcome' | 'latencyMs'> | null,
  testing: boolean,
  t?: TranslateFn,
): string {
  if (testing) {
    return t ? t('routes.tokens.testing') : '测试中…';
  }
  if (!result) {
    return t ? t('routes.tokens.testNoOutput') : '没有返回';
  }
  const label = localTokenTestResultLabel(result, t);
  if (result.outcome === 'ok') return label;
  const duration = localTokenTestDurationLabel(result.latencyMs, t);
  if (!duration) return label;
  const durationLabel = t ? t('routes.tokens.testDuration') : '耗时';
  return `${label} · ${durationLabel} ${duration}`;
}

export function localTokenTestDurationLabel(
  latencyMs: number | null | undefined,
  t?: TranslateFn,
): string {
  if (latencyMs == null) return '';
  const ms = Math.max(0, Math.round(latencyMs));
  return t ? t('routes.tokens.testDurationMs', { ms: String(ms) }) : `${ms}ms`;
}
