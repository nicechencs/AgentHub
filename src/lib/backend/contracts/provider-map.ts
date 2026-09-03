import type { AgentKey, Provider } from '@/lib/types';

export interface CoreProvider {
  id: string;
  agentId: AgentKey;
  name: string;
  settingsConfig: Record<string, unknown>;
  meta: Record<string, unknown>;
  isCurrent: boolean;
  createdAt?: string;
  updatedAt?: string;
}

export interface CoreProviderInput {
  id: string;
  agentId: AgentKey;
  name: string;
  settingsConfig: Record<string, unknown>;
  meta: Record<string, unknown>;
  isCurrent: boolean;
}

export interface CoreSwitchResult {
  provider: CoreProvider;
  backup?: {
    id: string;
    path?: string;
  } | null;
  backfilledProviderId?: string | null;
}

/** 与 src-tauri provider 命令 REDACTED_MARKER 一致 */
const REDACTED_MARKER = '***';

/**
 * When meta.preset is missing, do **not** invent the first catalog preset.
 * That lied for Kimi (first preset is kimi-code-membership) and made Adapter
 * look like a membership source while core classify saw Other.
 */
function displayPresetId(metaPreset: string | undefined): string {
  if (metaPreset && metaPreset.trim()) return metaPreset.trim();
  return 'custom';
}

function extractTomlContent(raw: Record<string, unknown>): string {
  // AgentHub: content；兼容别名: config
  if (typeof raw.content === 'string') return raw.content;
  if (typeof raw.config === 'string') return raw.config;
  return '';
}

function extractAuthApiKey(raw: Record<string, unknown>): string | undefined {
  const auth = raw.auth;
  if (!auth || typeof auth !== 'object' || Array.isArray(auth)) return undefined;
  const key = (auth as Record<string, unknown>).OPENAI_API_KEY;
  return typeof key === 'string' ? key : undefined;
}

/** Core Provider → UI Provider（脱敏配置文本） */
export function mapCoreProvider(p: CoreProvider): Provider {
  const raw = p.settingsConfig ?? {};
  // AgentHub TOML 包装：{ format: "toml", content }; 别名 config
  const isToml =
    raw.format === 'toml' ||
    (typeof raw.content === 'string' && raw.format !== 'json') ||
    typeof raw.config === 'string';
  const configFormat: 'json' | 'toml' = isToml ? 'toml' : 'json';

  let configText: string;
  if (configFormat === 'toml') {
    configText = extractTomlContent(raw);
  } else {
    configText = JSON.stringify(raw, null, 2);
  }

  const metaPreset = p.meta && typeof p.meta.preset === 'string' ? p.meta.preset : undefined;
  const authApiKey = extractAuthApiKey(raw);
  const official =
    p.meta && typeof p.meta.official === 'boolean' ? p.meta.official : undefined;
  const secretTail =
    p.meta && typeof p.meta.secretTail === 'string' && p.meta.secretTail.trim()
      ? p.meta.secretTail.trim()
      : undefined;
  const secretHash =
    p.meta && typeof p.meta.secretHash === 'string' && p.meta.secretHash.trim()
      ? p.meta.secretHash.trim()
      : undefined;

  return {
    id: p.id,
    agentId: p.agentId,
    name: p.name,
    preset: displayPresetId(metaPreset),
    configText,
    configFormat,
    authApiKey,
    isCurrent: p.isCurrent,
    updatedAt: p.updatedAt,
    official,
    secretTail,
    secretHash,
    home: p.meta && p.meta.home === 'route_pool' ? 'route_pool' : undefined,
  };
}

/** UI Provider → Core upsert input */
export function toCoreInput(p: Provider): CoreProviderInput {
  let settingsConfig: Record<string, unknown>;
  if (p.configFormat === 'toml') {
    settingsConfig = { format: 'toml', content: p.configText };
    // Codex dual shape: auth.OPENAI_API_KEY → live auth on switch
    if (p.authApiKey != null && p.authApiKey.trim()) {
      settingsConfig.auth = { OPENAI_API_KEY: p.authApiKey.trim() };
    } else if (
      p.agentId === 'codex' &&
      (p.authApiKey === '' || p.authApiKey === REDACTED_MARKER)
    ) {
      settingsConfig.auth = { OPENAI_API_KEY: REDACTED_MARKER };
    }
  } else {
    try {
      const parsed = JSON.parse(p.configText || '{}') as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        throw new Error('配置必须是 JSON 对象');
      }
      settingsConfig = parsed as Record<string, unknown>;
    } catch (e) {
      throw new Error(
        e instanceof Error && e.message.includes('JSON')
          ? e.message
          : `配置 JSON 无效: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }
  return {
    id: p.id,
    agentId: p.agentId,
    name: p.name,
    settingsConfig,
    // Surface is persisted core metadata; surface-less UI updates let core
    // inherit existing/future values and classify only newly created rows.
    meta: {
      preset: p.preset,
      // 产品：API Key 官方/自定义端点
      ...(p.official !== undefined ? { official: p.official } : {}),
      ...(p.home ? { home: p.home } : {}),
    },
    isCurrent: p.isCurrent,
  };
}
