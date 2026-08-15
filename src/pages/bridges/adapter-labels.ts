import {
  AdapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterBridgeRuntimeState,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfile,
  type AdapterProfileStatus,
} from '@/lib/backend/contracts/adapter';

export function adapterProfileStatusLabel(status: AdapterProfileStatus): string {
  if (status === 'active') return '已生效';
  if (status === 'applying') return '应用中';
  return '需要处理';
}
export function adapterBridgeStateLabel(state: AdapterBridgeRuntimeState | undefined): string {
  if (state === 'running') return '运行中';
  if (state === 'starting') return '启动中';
  if (state === 'stopping') return '停止中';
  if (state === 'error') return '运行错误';
  if (state === 'degraded') return '服务降级';
  return '已停止';
}

export function adapterBridgeUpstreamLabel(status: AdapterBridgeRuntimeStatus['upstreamStatus']): string {
  if (status === 'connected') return '已连接';
  if (status === 'stopped') return '已停止';
  if (status === 'degraded') return '降级';
  if (status === 'unavailable') return '不可用';
  return '未知';
}
export function adapterBridgeEndpointLabel(
  profile: AdapterProfile,
  status?: AdapterBridgeRuntimeStatus,
): string | null {
  const port = status?.port ?? profile.localPort;
  return port ? `127.0.0.1:${port}` : null;
}

export function bridgeStatusBadge(state: AdapterBridgeRuntimeState | undefined): {
  label: string;
  variant: 'success' | 'warning' | 'default';
} {
  return {
    label: adapterBridgeStateLabel(state),
    variant: state === 'running'
      ? 'success'
      : state === 'error' || state === 'degraded'
        ? 'warning'
        : 'default',
  };
}

export function profileStatusBadge(status: AdapterProfileStatus): { label: string; variant: 'success' | 'warning' | 'default' } {
  return {
    label: adapterProfileStatusLabel(status),
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

export function adapterErrorRetryHint(error: unknown): string | null {
  return isAdapterErrorRetryable(error) ? '此错误可重试。' : null;
}

export function adapterFailurePresentation(error: unknown, fallback: string): {
  message: string;
  retryable: boolean;
  hint: string;
} {
  const retryable = isAdapterErrorRetryable(error);
  return {
    message: errorMessage(error, fallback),
    retryable,
    hint: retryable
      ? '可重试；不会自动反复重试。'
      : '不可重试。检查来源连接，或删除后重建。',
  };
}
