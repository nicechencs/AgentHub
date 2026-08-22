import {
  AdapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterBridgeRuntimeState,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfile,
  type AdapterProfileStatus,
} from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';

export function adapterProfileStatusLabel(status: AdapterProfileStatus, t?: TranslateFn): string {
  if (status === 'active') return t ? t('routes.profileStatus.active') : '已生效';
  if (status === 'applying') return t ? t('routes.profileStatus.applying') : '应用中';
  return t ? t('routes.profileStatus.needsAttention') : '需要处理';
}
export function adapterBridgeStateLabel(state: AdapterBridgeRuntimeState | undefined, t?: TranslateFn): string {
  if (state === 'running') return t ? t('routes.bridgeState.running') : '运行中';
  if (state === 'starting') return t ? t('routes.bridgeState.starting') : '启动中';
  if (state === 'stopping') return t ? t('routes.bridgeState.stopping') : '停止中';
  if (state === 'error') return t ? t('routes.bridgeState.error') : '运行错误';
  if (state === 'degraded') return t ? t('routes.bridgeState.degraded') : '服务降级';
  return t ? t('routes.bridgeState.stopped') : '已停止';
}

export function adapterBridgeUpstreamLabel(
  status: AdapterBridgeRuntimeStatus['upstreamStatus'],
  t?: TranslateFn,
): string {
  if (status === 'connected') return t ? t('routes.upstream.connected') : '已连接';
  if (status === 'stopped') return t ? t('routes.upstream.stopped') : '已停止';
  if (status === 'degraded') return t ? t('routes.upstream.degraded') : '降级';
  if (status === 'unavailable') return t ? t('routes.upstream.unavailable') : '不可用';
  return t ? t('routes.upstream.unknown') : '未知';
}
export function adapterBridgeHostPort(
  profile: AdapterProfile,
  status?: AdapterBridgeRuntimeStatus,
): { host: '127.0.0.1'; port: number | null } {
  const raw = status?.port ?? profile.localPort;
  const port = typeof raw === 'number' && raw > 0 ? raw : null;
  return { host: '127.0.0.1', port };
}

export function adapterBridgeEndpointLabel(
  profile: AdapterProfile,
  status?: AdapterBridgeRuntimeStatus,
): string | null {
  const { host, port } = adapterBridgeHostPort(profile, status);
  return port ? `${host}:${port}` : null;
}

export function bridgeStatusBadge(state: AdapterBridgeRuntimeState | undefined, t?: TranslateFn): {
  label: string;
  variant: 'success' | 'warning' | 'default';
} {
  return {
    label: adapterBridgeStateLabel(state, t),
    variant: state === 'running'
      ? 'success'
      : state === 'error' || state === 'degraded'
        ? 'warning'
        : 'default',
  };
}

export function profileStatusBadge(
  status: AdapterProfileStatus,
  t?: TranslateFn,
): { label: string; variant: 'success' | 'warning' | 'default' } {
  return {
    label: adapterProfileStatusLabel(status, t),
    variant: status === 'active' ? 'success' : status === 'needs_attention' ? 'warning' : 'default',
  };
}

export function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof AdapterCommandError && error.message.trim()) return error.message;
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === 'string' && error.trim()) return error;
  return fallback;
}

export function isAdapterErrorRetryable(error: unknown): boolean {
  if (error instanceof AdapterCommandError) return error.retryable;
  if (error && typeof error === 'object' && 'retryable' in error && typeof error.retryable === 'boolean') {
    return error.retryable;
  }
  if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string') {
    return isAdapterErrorCodeRetryable(error.code);
  }
  return false;
}

export function adapterErrorDetails(error: unknown): string | null {
  if (error instanceof AdapterCommandError) {
    const details = error.details?.trim();
    return details || null;
  }
  if (error && typeof error === 'object' && 'details' in error && typeof error.details === 'string') {
    const details = error.details.trim();
    return details || null;
  }
  return null;
}

export function adapterErrorRetryHint(error: unknown, t?: TranslateFn): string | null {
  return isAdapterErrorRetryable(error) ? (t ? t('routes.retryHint') : '此错误可重试。') : null;
}

export function adapterFailurePresentation(error: unknown, fallback: string, t?: TranslateFn): {
  message: string;
  retryable: boolean;
  hint: string;
} {
  const retryable = isAdapterErrorRetryable(error);
  return {
    message: errorMessage(error, fallback),
    retryable,
    hint: retryable
      ? (t ? t('routes.failureRetryable') : '可重试；不会自动反复重试。')
      : (t ? t('routes.failureNotRetryable') : '不可重试。检查来源连接，或删除后重建。'),
  };
}
