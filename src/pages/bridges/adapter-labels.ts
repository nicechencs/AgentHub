import {
  AdapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterBridgeRuntimeState,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfile,
  type AdapterProfileStatus,
} from '@/lib/backend/contracts/adapter';
import type { MessageKey, TranslateFn } from '@/lib/i18n';

const ADAPTER_ERROR_KEY = {
  invalidProjection: 'routes.error.invalidProjection',
  portInUse: 'routes.error.portInUse',
  unsupportedBridge: 'routes.error.unsupportedBridge',
  unknownRelay: 'routes.error.unknownRelay',
  invalidSecret: 'routes.error.invalidSecret',
  cannotListen: 'routes.error.cannotListen',
  cannotStart: 'routes.error.cannotStart',
  grokKey: 'routes.error.grokKey',
  loginKey: 'routes.error.loginKey',
} as const satisfies Record<string, MessageKey>;

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
export function adapterBridgeIsListening(status?: AdapterBridgeRuntimeStatus): boolean {
  return status?.state === 'running'
    || status?.state === 'starting'
    || status?.state === 'degraded';
}

export function adapterBridgeHostPort(
  profile: AdapterProfile,
  status?: AdapterBridgeRuntimeStatus,
): { host: '127.0.0.1'; port: number | null } {
  if (!adapterBridgeIsListening(status)) {
    return { host: '127.0.0.1', port: null };
  }
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

function localizeAdapterCopy(raw: string, t?: TranslateFn): string {
  const trimmed = raw.trim();
  const copy = (key: keyof typeof ADAPTER_ERROR_KEY, zh: string) =>
    t ? t(ADAPTER_ERROR_KEY[key]) : zh;
  if (
    /generated bridge provider has an invalid projection/i.test(trimmed)
    || trimmed.includes('配置不完整')
  ) {
    return copy('invalidProjection', '这条本机路由的配置不完整，无法启动。请点重试，或删除后重建。');
  }
  if (
    /failed to bind loopback bridge listener|address already in use/i.test(trimmed)
    || trimmed.includes('端口已被占用')
  ) {
    return copy('portInUse', '本机端口已被占用。将自动换一个空闲端口，请点重试。');
  }
  if (
    /adapter profile is not a supported local bridge/i.test(trimmed)
    || trimmed.includes('本机路由已失效')
  ) {
    return copy('unsupportedBridge', '这条本机路由已失效，无法启动。请删除后重建。');
  }
  if (
    /unknown custom relay provider/i.test(trimmed)
    || trimmed.includes('自定义上游还缺')
  ) {
    return copy('unknownRelay', '这份自定义上游还缺有效的服务地址，没法开本机转发。请补上地址后重试。');
  }
  if (/invalid adapter secret reference/i.test(trimmed) || trimmed.includes('本机令牌写进')) {
    return copy('invalidSecret', '没法把本机令牌写进客户端配置。请点重试。');
  }
  if (trimmed.includes('无法监听端口') || /couldn't listen|cannot listen/i.test(trimmed)) {
    return copy('cannotListen', '本机转发无法监听端口，请点重试。');
  }
  if (
    trimmed.includes('无法启动或停止')
    || trimmed.includes('本机转发启动失败')
    || /failed to start or stop|bridge start/i.test(trimmed)
  ) {
    return copy('cannotStart', '本机转发无法启动或停止，请点重试。');
  }
  if (trimmed.includes('Grok 登录没法解析') || /grok login couldn.?t/i.test(trimmed)) {
    return copy('grokKey', '这份 Grok 登录没法解析成 Claude 路由要用的密钥');
  }
  if (trimmed.includes('没法解析成目标路由') || /couldn.?t be turned into the key/i.test(trimmed)) {
    return copy('loginKey', '这份登录没法解析成目标路由要用的密钥');
  }
  return trimmed.replace(INTERNAL_ID_RE, '').replace(/\s{2,}/g, ' ').trim();
}

export function errorMessage(error: unknown, fallback: string, t?: TranslateFn): string {
  const raw = error instanceof AdapterCommandError && error.message.trim()
    ? error.message
    : error instanceof Error && error.message.trim()
      ? error.message
      : typeof error === 'string' && error.trim()
        ? error
        : '';
  if (!raw) return fallback;
  const localized = localizeAdapterCopy(raw, t);
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

export function adapterErrorDetails(error: unknown, t?: TranslateFn): string | null {
  let details: string | null = null;
  if (error instanceof AdapterCommandError) {
    details = error.details?.trim() || null;
  } else if (error && typeof error === 'object' && 'details' in error && typeof error.details === 'string') {
    details = error.details.trim() || null;
  }
  if (!details) return null;
  const cleaned = localizeAdapterCopy(details, t);
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
    message: errorMessage(error, fallback, t),
    retryable,
    hint: retryable
      ? (t ? t('routes.failureRetryable') : '可重试；不会自动反复重试。')
      : (t ? t('routes.failureNotRetryable') : '不可重试。检查来源连接，或删除后重建。'),
  };
}
