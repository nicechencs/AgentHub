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

const INTERNAL_ID_RE =
  /\b(?:adapter-[a-z0-9-]+|retryable:adapter\.[a-z0-9._-]+|adapter\.[a-z0-9._-]+|[a-z0-9-]+-to-[a-z0-9-]+-v\d+)\b/gi;

function localizeAdapterCopy(raw: string): string {
  const trimmed = raw.trim();
  if (/generated bridge provider has an invalid projection/i.test(trimmed)) {
    return '这条本机路由的配置不完整，无法启动。请点重试，或删除后重建。';
  }
  if (/failed to bind loopback bridge listener|address already in use/i.test(trimmed)) {
    return '本机端口已被占用。将自动换一个空闲端口，请点重试。';
  }
  if (/adapter profile is not a supported local bridge/i.test(trimmed)) {
    return '这条本机路由已失效，无法启动。请删除后重建。';
  }
  if (/unknown custom relay provider/i.test(trimmed)) {
    return '这份自定义上游还缺有效的服务地址，没法开本机转发。请补上地址后重试。';
  }
  return trimmed.replace(INTERNAL_ID_RE, '').replace(/\s{2,}/g, ' ').trim();
}

export function errorMessage(error: unknown, fallback: string): string {
  const raw = error instanceof AdapterCommandError && error.message.trim()
    ? error.message
    : error instanceof Error && error.message.trim()
      ? error.message
      : typeof error === 'string' && error.trim()
        ? error
        : '';
  if (!raw) return fallback;
  const localized = localizeAdapterCopy(raw);
  return localized || fallback;
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
  let details: string | null = null;
  if (error instanceof AdapterCommandError) {
    details = error.details?.trim() || null;
  } else if (error && typeof error === 'object' && 'details' in error && typeof error.details === 'string') {
    details = error.details.trim() || null;
  }
  if (!details) return null;
  const cleaned = localizeAdapterCopy(details);
  if (!cleaned || /adapter-|retryable:|wire_api|projection|loopback|PKCE/i.test(cleaned)) {
    return null;
  }
  return cleaned;
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
