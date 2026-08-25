/**
 * 将账号池 / API 供应商池合并为 Dashboard 可用的「当前生效连接」。
 * 数据源是 AgentHub SQLite 中 isCurrent 行（切换时写入），不是演示 mock。
 */
import { logger } from '@/lib/logger';
import {
  authDisplayForAccount,
  authDisplayForAgentStatus,
  authHealthLabel,
  normalizeAuthHealth,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import type { TranslateFn } from '@/lib/i18n';
import { truncateAtWord } from '@/lib/text-truncate';
import type {
  Account,
  AgentStatus,
  AuthStatus,
  EffectiveConnectionKind,
  Provider,
} from '@/lib/types';

const log = logger.scope('contracts:agent-connection');

export interface EffectiveConnection {
  kind: EffectiveConnectionKind;
  /** 卡片主展示：账号 label 或「供应商 · host」 */
  label: string;
  authLabel: string;
  authStatus: AuthStatus;
  authHealth: AuthHealth;
  authHealthLabel: string;
}

/** 从脱敏配置文本里尽量抽出 endpoint / base_url（无密钥） */
export function extractProviderEndpoint(
  configText: string,
  format: 'json' | 'toml',
): string | undefined {
  const text = configText?.trim() ?? '';
  if (!text) return undefined;

  if (format === 'json') {
    try {
      const parsed = JSON.parse(text) as Record<string, unknown>;
      const env =
        parsed.env && typeof parsed.env === 'object' && !Array.isArray(parsed.env)
          ? (parsed.env as Record<string, unknown>)
          : parsed;
      const bags: Record<string, unknown>[] = env === parsed ? [env] : [env, parsed];
      for (const bag of bags) {
        for (const key of [
          'ANTHROPIC_BASE_URL',
          'OPENAI_BASE_URL',
          'OPENAI_API_BASE',
          'baseURL',
          'baseUrl',
          'base_url',
          'BASE_URL',
          'api_base',
          'apiBase',
        ]) {
          const v = bag[key];
          if (typeof v === 'string' && /^https?:\/\//i.test(v.trim())) {
            return v.trim();
          }
        }
      }
    } catch {
      // fall through to regex
    }
  }

  const tomlBase = text.match(/(?:^|\n)\s*base_url\s*=\s*["']([^"']+)["']/i);
  if (tomlBase?.[1] && /^https?:\/\//i.test(tomlBase[1])) {
    return tomlBase[1];
  }

  const envUrl = text.match(
    /(?:ANTHROPIC_BASE_URL|OPENAI_BASE_URL|OPENAI_API_BASE)\s*[=:]\s*["']?(https?:\/\/[^\s"',\\]+)/i,
  );
  if (envUrl?.[1]) return envUrl[1];

  const any = text.match(/https?:\/\/[^\s"',\\]+/i);
  return any?.[0];
}

const LOOPBACK_HOSTS = new Set(['localhost', '127.0.0.1', '::1', '0.0.0.0']);

/** Internal adapter-generated / live-pool ids that must never be a title. */
const INTERNAL_ID_RE = /grok-live-|-(?:adapter|bridge)-|^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isLoopbackUrl(url: string): boolean {
  try {
    const host = new URL(url).hostname.replace(/^\[|\]$/g, '').toLowerCase();
    return LOOPBACK_HOSTS.has(host);
  } catch {
    return /127\.0\.0\.1|localhost|\[::1\]/i.test(url);
  }
}

/** True for English bridge product names, grok-live-* ids, raw uuids. */
export function isInternalGeneratedName(value: string | undefined | null): boolean {
  const v = value?.trim() ?? '';
  if (!v) return false;
  if (/bridge/i.test(v)) return true;
  if (INTERNAL_ID_RE.test(v)) return true;
  return false;
}

export function isInternalGeneratedProvider(
  provider: Pick<Provider, 'id' | 'name' | 'configText' | 'configFormat'>,
): boolean {
  if (isInternalGeneratedName(provider.name) || isInternalGeneratedName(provider.id)) {
    return true;
  }
  const endpoint = extractProviderEndpoint(provider.configText, provider.configFormat);
  return Boolean(endpoint && isLoopbackUrl(endpoint));
}

export function formatLocalRouteLabel(sourceLabel?: string, t?: TranslateFn): string {
  const base = t ? t('kind.route.localRoute') : '本机路由';
  const src = sourceLabel?.trim() ?? '';
  if (!src || isInternalGeneratedName(src) || /:\d{2,5}\b/.test(src)) return base;
  return `${base} · ${src}`;
}

/** 缩短 URL 便于卡片一行展示。Never emit host:port; never cut mid-word. */
export function formatEndpointHost(url: string): string {
  try {
    const u = new URL(url);
    const path = u.pathname === '/' ? '' : u.pathname.replace(/\/$/, '');
    const hostPath = `${u.hostname}${path}`;
    return hostPath.length > 36 ? truncateAtWord(hostPath, 33) : hostPath;
  } catch {
    return url.length > 36 ? truncateAtWord(url, 33) : url;
  }
}

export type FormatApiConnectionLabelOptions = {
  t?: TranslateFn;
  sourceLabel?: string;
};

export function formatApiConnectionLabel(
  provider: Provider,
  options?: FormatApiConnectionLabelOptions,
): string {
  if (isInternalGeneratedProvider(provider)) {
    return formatLocalRouteLabel(options?.sourceLabel, options?.t);
  }
  const endpoint = extractProviderEndpoint(provider.configText, provider.configFormat);
  if (endpoint) {
    if (isLoopbackUrl(endpoint)) {
      return formatLocalRouteLabel(options?.sourceLabel, options?.t);
    }
    return `${provider.name} · ${formatEndpointHost(endpoint)}`;
  }
  return provider.name;
}

function authStatusFromAccount(account: Account): AuthStatus {
  return authDisplayForAccount(account).legacyStatus;
}

function accountAuthLabel(account: Account): string {
  if (account.kind === 'oauth') {
    return account.subscription?.trim() || '已登录';
  }
  return 'API Key';
}

function sortKey(updatedAt?: string, lastUsedAt?: string): string {
  return updatedAt || lastUsedAt || '';
}

/**
 * 在「当前账号」与「当前供应商」之间选出一项作为生效展示。
 * 两者都有 isCurrent 时，取 updatedAt 更新更晚的一方（最近一次切换）。
 */
export function resolveEffectiveConnection(
  account: Account | undefined,
  provider: Provider | undefined,
  options?: FormatApiConnectionLabelOptions,
): EffectiveConnection {
  if (account && provider) {
    const accKey = sortKey(account.updatedAt, account.lastUsedAt);
    const provKey = sortKey(provider.updatedAt);
    // 时间相等时优先账号（订阅登录语义更贴近「当前登录」）
    if (provKey > accKey) {
      return {
        kind: 'api',
        label: formatApiConnectionLabel(provider, options),
        authLabel: 'API',
        authStatus: 'valid',
        authHealth: 'configured',
        authHealthLabel: authHealthLabel('configured'),
      };
    }
    return {
      kind: 'account',
      label: account.label,
      authLabel: accountAuthLabel(account),
      authStatus: authStatusFromAccount(account),
      authHealth: authDisplayForAccount(account).health,
      authHealthLabel: authDisplayForAccount(account).label,
    };
  }

  if (account) {
    return {
      kind: 'account',
      label: account.label,
      authLabel: accountAuthLabel(account),
      authStatus: authStatusFromAccount(account),
      authHealth: authDisplayForAccount(account).health,
      authHealthLabel: authDisplayForAccount(account).label,
    };
  }

  if (provider) {
    return {
      kind: 'api',
      label: formatApiConnectionLabel(provider, options),
      authLabel: 'API',
      authStatus: 'valid',
      authHealth: 'configured',
      authHealthLabel: authHealthLabel('configured'),
    };
  }

  return {
    kind: 'none',
    label: '未配置',
    authLabel: '未配置',
    authStatus: 'none',
    authHealth: 'missing',
    authHealthLabel: authHealthLabel('missing'),
  };
}

/** 把生效连接写回 AgentStatus（不改安装/版本等 detect 字段） */
export function applyEffectiveConnection(
  status: AgentStatus,
  account: Account | undefined,
  provider: Provider | undefined,
): AgentStatus {
  if (!status.installed) {
    return {
      ...status,
      effectiveKind: 'none',
      effectiveLabel: undefined,
      currentProvider: undefined,
      authStatus: 'none',
      authLabel: '未配置',
      authHealth: 'missing',
      authHealthLabel: authHealthLabel('missing'),
    };
  }

  const eff = resolveEffectiveConnection(account, provider);
  // The shared AgentStatus store attaches this from probe_live_auth. Account
  // rows describe the saved pool, while this value describes the credentials
  // actually present in the agent's live configuration right now.
  const liveHealth = normalizeAuthHealth(status.authHealth);
  const liveDisplay = liveHealth ? authDisplayForAgentStatus(status) : undefined;
  return {
    ...status,
    effectiveKind: eff.kind,
    effectiveLabel: eff.label,
    // 兼容旧字段名：Dashboard 等曾用 currentProvider 表示副标题左侧
    currentProvider: eff.kind === 'none' ? undefined : eff.label,
    authStatus: liveDisplay?.legacyStatus ?? eff.authStatus,
    authLabel: liveDisplay?.label ?? eff.authLabel,
    authHealth: liveDisplay?.health ?? eff.authHealth,
    authHealthLabel: liveDisplay?.label ?? eff.authHealthLabel,
  };
}

export function enrichStatusesWithConnections(
  agents: AgentStatus[],
  accounts: Account[],
  providers: Provider[],
): AgentStatus[] {
  const currentAccountByAgent = new Map<string, Account>();
  for (const a of accounts) {
    if (!a.isCurrent) continue;
    // 同 agent 多条 isCurrent 时保留后写（与 DB 约束冲突时仍可诊断）
    if (currentAccountByAgent.has(a.agentId)) {
      log.warn('multiple isCurrent accounts for agent; using last', {
        agentId: a.agentId,
        previousId: currentAccountByAgent.get(a.agentId)?.id,
        nextId: a.id,
      });
    }
    currentAccountByAgent.set(a.agentId, a);
  }
  const currentProviderByAgent = new Map<string, Provider>();
  for (const p of providers) {
    if (!p.isCurrent) continue;
    if (currentProviderByAgent.has(p.agentId)) {
      log.warn('multiple isCurrent providers for agent; using last', {
        agentId: p.agentId,
        previousId: currentProviderByAgent.get(p.agentId)?.id,
        nextId: p.id,
      });
    }
    currentProviderByAgent.set(p.agentId, p);
  }

  const enriched = agents.map((s) =>
    applyEffectiveConnection(
      s,
      currentAccountByAgent.get(s.agentId),
      currentProviderByAgent.get(s.agentId),
    ),
  );

  const installed = enriched.filter((a) => a.installed);
  log.debug('enriched agent connections', {
    poolAccounts: accounts.length,
    poolProviders: providers.length,
    currentAccounts: currentAccountByAgent.size,
    currentProviders: currentProviderByAgent.size,
    installed: installed.length,
  });

  return enriched;
}
