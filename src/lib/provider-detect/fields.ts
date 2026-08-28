/**
 * 供应商表单字段 ↔ 配置原文（JSON/TOML）读写。
 * 与 detectors 同属 provider-detect，供编辑弹窗统一调用。
 */
import type { AgentId } from '@/lib/types';
import {
  defaultPiProviderApi,
  isPiAuthJsonSlot,
} from '@/lib/pi-provider-slots';
import {
  claudeContextWindowFor,
  contextWindowTokensFromChoice,
  parseContextWindowChoice,
} from '@/lib/claude-client-env';
import {
  firstNonEmptyString,
  stripClaudeForeignRootKeys,
} from './native-config';
import {
  CLAUDE_MODEL_ROLE_ENV,
  EMPTY_FORM_VARS,
  REDACTED_MARKER,
  type FormFieldKey,
  type ProviderFormVars,
} from './types';

export { EMPTY_FORM_VARS, REDACTED_MARKER, CLAUDE_MODEL_ROLE_ENV };
export type { FormFieldKey, ProviderFormVars };

/** 脱敏 / 占位 / 后端 redaction marker */
export function looksRedactedOrPlaceholder(v: string): boolean {
  if (!v) return false;
  if (v === REDACTED_MARKER) return true;
  return /[•…]|\*{3,}|xxxx|XXXX|xxxxxxxx|your-api-key|sk-x{4,}/i.test(v);
}

function sanitizeSecretForForm(v: string): string {
  return looksRedactedOrPlaceholder(v) ? '' : v;
}

/** Empty / `***` / last4 masks are never a new secret. */
export function writableSecret(v: string): string {
  const trimmed = v.trim();
  return looksRedactedOrPlaceholder(trimmed) ? '' : trimmed;
}

/**
 * Advanced-editor overlay: `***` (or other masks) keep the previous form
 * secret instead of becoming the new API Key.
 */
export function resolveFormApiKeyFromEditor(
  extracted: string,
  detected: string,
  previous: string,
): string {
  const next = extracted || detected || '';
  if (!next || looksRedactedOrPlaceholder(next)) return previous;
  return next;
}

const JSON_SECRET_KEYS = new Set([
  'apikey',
  'api_key',
  'anthropic_api_key',
  'anthropic_auth_token',
  'openai_api_key',
  'cursor_api_key',
]);

function isJsonSecretKey(key: string, value: string): boolean {
  if (JSON_SECRET_KEYS.has(key.toLowerCase())) return true;
  if (key !== 'key' || looksRedactedOrPlaceholder(value) || value.length < 16) {
    return false;
  }
  return /^(sk-|cr_|ak-|xai-|sk-ant-)/i.test(value);
}

function redactJsonSecrets(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(redactJsonSecrets);
  if (!value || typeof value !== 'object') return value;
  const obj = value as Record<string, unknown>;
  const out: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(obj)) {
    if (typeof child === 'string' && child && isJsonSecretKey(key, child)) {
      out[key] = REDACTED_MARKER;
    } else {
      out[key] = redactJsonSecrets(child);
    }
  }
  return out;
}

const JSON_SECRET_TEXT_RE =
  /("(?:ANTHROPIC_AUTH_TOKEN|ANTHROPIC_API_KEY|OPENAI_API_KEY|CURSOR_API_KEY|apiKey|api_key)"\s*:\s*)"(?:\\.|[^"\\])*"/gi;
const TOML_API_KEY_LINE_RE = /^(\s*api_key\s*=\s*)(["']).*?\2/gim;

/**
 * Mask secret values in the advanced editor (`***`). Never invent a live key.
 * `env_key` names are left intact.
 */
const SECRET_ASSIGNMENT_RE =
  /^(\s*(?:export\s+|set\s+|\$env:)?[A-Za-z_][A-Za-z0-9_]*(?:_API_KEY|_AUTH_TOKEN|_ACCESS_TOKEN|_TOKEN|_SECRET|API_KEY)\s*=\s*)(.+)$/gim;

/** Mask KEY=value secret assignments in free-form paste (env blocks, shell). */
export function maskPasteSecrets(text: string): string {
  const trimmed = text.trim();
  if (!trimmed || trimmed === REDACTED_MARKER) return text;
  return text.replace(SECRET_ASSIGNMENT_RE, `$1${REDACTED_MARKER}`);
}

export function maskConfigSecrets(
  _agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
): string {
  const trimmed = configText.trim();
  if (!trimmed || trimmed === REDACTED_MARKER) return configText;
  if (format === 'toml') {
    const withoutExports = configText
      .split('\n')
      .filter((line) => {
        const trimmed = line.trim();
        return !/^(export\s+|set\s+|\$env:)?[A-Za-z_][A-Za-z0-9_]*(_API_KEY|_AUTH_TOKEN|_TOKEN|_SECRET|API_KEY)\s*=/i.test(
          trimmed,
        );
      })
      .join('\n');
    return withoutExports.replace(TOML_API_KEY_LINE_RE, `$1"${REDACTED_MARKER}"`);
  }
  const parsed = parseJsonObjectConfig(configText);
  if (parsed.ok) {
    return JSON.stringify(redactJsonSecrets(parsed.value), null, 2);
  }
  return maskPasteSecrets(configText.replace(JSON_SECRET_TEXT_RE, `$1"${REDACTED_MARKER}"`));
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

export type JsonObjectParseResult =
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; message: string };

/** Parse a structured JSON config without ever substituting an empty object. */
export function parseJsonObjectConfig(configText: string): JsonObjectParseResult {
  const trimmed = configText.trim();
  if (!trimmed) return { ok: true, value: {} };
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return { ok: false, message: '配置 JSON 必须是对象' };
    }
    return { ok: true, value: parsed as Record<string, unknown> };
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e);
    return { ok: false, message: `配置 JSON 解析失败：${detail}` };
  }
}

function firstPiAuthApiKey(
  auth: Record<string, unknown> | undefined,
  preferSlug?: string,
): { slug: string; key: string } | undefined {
  if (!auth) return undefined;
  const readKey = (entry: unknown): string => {
    const obj = objectValue(entry);
    const raw =
      (typeof obj?.key === 'string' && obj.key) ||
      (typeof obj?.api_key === 'string' && obj.api_key) ||
      (typeof obj?.apiKey === 'string' && obj.apiKey) ||
      '';
    return sanitizeSecretForForm(raw);
  };
  if (preferSlug) {
    const key = readKey(auth[preferSlug]);
    if (key) return { slug: preferSlug, key };
  }
  for (const [slug, entry] of Object.entries(auth)) {
    const obj = objectValue(entry);
    const type = typeof obj?.type === 'string' ? obj.type : '';
    const key = readKey(entry);
    if (key && (type === 'api_key' || type === 'apikey' || type === 'api-key' || !type)) {
      return { slug, key };
    }
  }
  return undefined;
}

function extractPiProviderVars(root: unknown): ProviderFormVars {
  const rootObject = objectValue(root);
  const modelsObject = objectValue(rootObject?.models);
  const providers = objectValue(modelsObject?.providers) ?? objectValue(rootObject?.providers);
  const entries = providers ? Object.entries(providers) : [];
  const preferred =
    entries.find(([, value]) => {
      const provider = objectValue(value);
      return typeof provider?.baseUrl === 'string' && provider.baseUrl.trim();
    }) ?? entries[0];
  const [providerSlug, providerValue] = preferred ?? [];
  const provider = objectValue(providerValue);
  const models = Array.isArray(provider?.models) ? provider.models : [];
  const model = objectValue(models[0]);
  const authHit = firstPiAuthApiKey(objectValue(rootObject?.auth), providerSlug);
  const providerKey = sanitizeSecretForForm(
    typeof provider?.apiKey === 'string' ? provider.apiKey : '',
  );
  return {
    ...EMPTY_FORM_VARS,
    providerSlug: providerSlug ?? authHit?.slug ?? 'custom',
    baseUrl: typeof provider?.baseUrl === 'string' ? provider.baseUrl : '',
    apiKey: providerKey || authHit?.key || '',
    model: typeof model?.id === 'string' ? model.id : '',
  };
}

function extractDshFormVars(root: Record<string, unknown>): ProviderFormVars {
  const env =
    root.env && typeof root.env === 'object' && !Array.isArray(root.env)
      ? (root.env as Record<string, unknown>)
      : {};
  const provider = firstNonEmptyString(root.provider) || 'deepseek-official';
  const baseUrl = firstNonEmptyString(
    root.baseUrl,
    root.baseURL,
    root.base_url,
    env.OPENAI_BASE_URL,
    env.ANTHROPIC_BASE_URL,
  ).replace(/\/anthropic\/?$/i, '');
  const apiKey = firstNonEmptyString(
    root.apiKey,
    root.api_key,
    env.OPENAI_API_KEY,
    env.DEEPSEEK_API_KEY,
    env.ANTHROPIC_AUTH_TOKEN,
    env.ANTHROPIC_API_KEY,
  );
  return {
    ...EMPTY_FORM_VARS,
    providerSlug: provider,
    baseUrl,
    apiKey: sanitizeSecretForForm(apiKey),
    model: firstNonEmptyString(root.model, env.ANTHROPIC_MODEL, env.MODEL),
  };
}

function applyDshFormVars(root: Record<string, unknown>, vars: ProviderFormVars): string {
  const next: Record<string, unknown> = { ...root };
  delete next.env;
  const slug = vars.providerSlug.trim() || 'deepseek-official';
  next.provider = slug;
  if (vars.model.trim()) next.model = vars.model.trim();
  else delete next.model;
  const url = vars.baseUrl.trim().replace(/\/anthropic\/?$/i, '');
  if (url) {
    next.baseUrl = url;
    next.baseURL = url;
  } else {
    delete next.baseUrl;
    delete next.baseURL;
  }
  const secret = writableSecret(vars.apiKey);
  if (secret) next.apiKey = secret;
  else if (typeof next.apiKey === 'string' || typeof next.api_key === 'string') {
    next.apiKey = REDACTED_MARKER;
  }
  delete next.api_key;
  return JSON.stringify(next, null, 2);
}

function extractWorkBuddyModelVars(root: unknown): ProviderFormVars {
  const rootObject = objectValue(root);
  const nestedModels = objectValue(rootObject?.models);
  const modelsValue = nestedModels?.models ?? rootObject?.models;
  const models = Array.isArray(modelsValue) ? modelsValue : [];
  const model = objectValue(models[0]);
  return {
    ...EMPTY_FORM_VARS,
    baseUrl: typeof model?.url === 'string' ? model.url : '',
    apiKey: sanitizeSecretForForm(typeof model?.apiKey === 'string' ? model.apiKey : ''),
    model: typeof model?.id === 'string' ? model.id : '',
  };
}

function applyPiProviderVars(root: Record<string, unknown>, vars: ProviderFormVars): string {
  const slug = vars.providerSlug.trim() || 'custom';
  const key = writableSecret(vars.apiKey);
  const url = vars.baseUrl.trim();
  const authSlot = isPiAuthJsonSlot(slug);
  // Official slots without a relay URL stay on Pi builtins (auth.json only).
  // A custom / bind slot, or an official slot with a URL, writes models.json.
  const writeModelsProvider = !authSlot || Boolean(url);
  const liveEnvelope = Boolean(objectValue(root.paths));

  const modelsObject = objectValue(root.models);
  const existingProviders =
    objectValue(modelsObject?.providers) ?? objectValue(root.providers) ?? {};
  const providers: Record<string, unknown> = { ...existingProviders };
  const ownedSlugs = Object.keys(providers);
  if (ownedSlugs.length === 1 && ownedSlugs[0] !== slug) {
    const prior = objectValue(providers[ownedSlugs[0]]);
    delete providers[ownedSlugs[0]];
    if (prior) providers[slug] = prior;
  }

  if (writeModelsProvider) {
    const existingProvider = objectValue(providers[slug]);
    const provider: Record<string, unknown> = existingProvider ? { ...existingProvider } : {};
    const models = Array.isArray(provider.models) ? [...provider.models] : [];
    const existingModel = objectValue(models[0]);
    const model: Record<string, unknown> = existingModel ? { ...existingModel } : {};
    const modelId = vars.model.trim() || (typeof model.id === 'string' ? model.id : 'custom-model');

    if (url) provider.baseUrl = url;
    if (key) provider.apiKey = key;
    else if (typeof provider.apiKey === 'string') provider.apiKey = REDACTED_MARKER;
    if (typeof provider.api !== 'string' || !provider.api) {
      provider.api = defaultPiProviderApi(slug);
    }
    model.id = modelId;
    if (typeof model.name !== 'string' || !model.name) model.name = modelId;
    models[0] = model;
    provider.models = models;
    providers[slug] = provider;
  } else {
    delete providers[slug];
  }

  const nextModels: Record<string, unknown> = modelsObject ? { ...modelsObject } : {};
  if (Object.keys(providers).length > 0) {
    nextModels.providers = providers;
    root.models = nextModels;
  } else if (modelsObject) {
    delete nextModels.providers;
    if (Object.keys(nextModels).length > 0) root.models = nextModels;
    else delete root.models;
  }
  delete root.providers;

  const existingAuth = objectValue(root.auth);
  if (authSlot && key) {
    root.auth = {
      ...(existingAuth ?? {}),
      [slug]: { type: 'api_key', key },
    };
  } else if (authSlot && existingAuth) {
    // Edit with empty key: keep the stored auth slot so write_config can merge.
    root.auth = existingAuth;
  } else if (!authSlot && existingAuth && !liveEnvelope) {
    const authKeys = Object.keys(existingAuth);
    if (authKeys.length === 1) {
      delete root.auth;
    }
  }

  return JSON.stringify(root, null, 2);
}

function applyWorkBuddyModelVars(root: Record<string, unknown>, vars: ProviderFormVars): string {
  const nestedModels = objectValue(root.models);
  const native = nestedModels && ('models' in nestedModels || 'availableModels' in nestedModels)
    ? nestedModels
    : root;
  const models = Array.isArray(native.models) ? [...native.models] : [];
  const existingModel = objectValue(models[0]);
  const model: Record<string, unknown> = existingModel ? { ...existingModel } : {};
  const oldId = typeof model.id === 'string' ? model.id : '';
  const modelId = vars.model.trim() || oldId || 'custom-model';

  model.id = modelId;
  if (typeof model.name !== 'string' || !model.name) model.name = modelId;
  if (vars.baseUrl.trim()) model.url = vars.baseUrl.trim();
  if (writableSecret(vars.apiKey)) model.apiKey = writableSecret(vars.apiKey);
  else if (typeof model.apiKey === 'string') model.apiKey = REDACTED_MARKER;
  models[0] = model;
  native.models = models;

  const available = Array.isArray(native.availableModels)
    ? native.availableModels.filter((id): id is string => typeof id === 'string')
    : [];
  const nextAvailable = available.map((id) => (oldId && id === oldId ? modelId : id));
  if (!nextAvailable.includes(modelId)) nextAvailable.push(modelId);
  native.availableModels = nextAvailable;
  if (native !== root) root.models = native;
  return JSON.stringify(root, null, 2);
}

function extractCursorFormVars(root: Record<string, unknown>): ProviderFormVars {
  return {
    ...EMPTY_FORM_VARS,
    baseUrl: firstNonEmptyString(root.baseUrl, root.base_url, root.baseURL),
    apiKey: sanitizeSecretForForm(
      firstNonEmptyString(root.apiKey, root.api_key, root.CURSOR_API_KEY),
    ),
    model: firstNonEmptyString(root.model),
  };
}

function applyCursorFormVars(
  root: Record<string, unknown>,
  vars: ProviderFormVars,
): string {
  // Pool-only: never project Claude ANTHROPIC_* env.
  delete root.env;
  if (vars.baseUrl.trim()) root.baseUrl = vars.baseUrl.trim();
  if (vars.model.trim()) root.model = vars.model.trim();
  const secret = writableSecret(vars.apiKey);
  if (secret) root.apiKey = secret;
  else if (typeof root.apiKey === 'string' || typeof root.api_key === 'string') {
    root.apiKey = REDACTED_MARKER;
  }
  if (Object.keys(root).length === 0) root.note = 'cursor-pool-only';
  return JSON.stringify(root, null, 2);
}

function tomlGet(text: string, key: string): string {
  const re = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=\\s*"([^"]*)"`, 'm');
  return text.match(re)?.[1] ?? '';
}

function tomlSet(text: string, key: string, value: string): string {
  const re = new RegExp(`^(\\s*${escapeRegExp(key)}\\s*=\\s*)"[^"]*"`, 'm');
  if (re.test(text)) return text.replace(re, `$1"${value}"`);
  // 插到第一个 [section] 之前；没有 section 则追加文末
  const sectionIdx = text.search(/^\[/m);
  const line = `${key} = "${value}"\n`;
  if (sectionIdx >= 0) {
    return text.slice(0, sectionIdx) + line + text.slice(sectionIdx);
  }
  const pad = text.endsWith('\n') || text.length === 0 ? '' : '\n';
  return `${text}${pad}${line}`;
}

function tomlUnset(text: string, key: string): string {
  const re = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=\\s*"[^"]*"\\s*(?:\\r?\\n)?`, 'm');
  return text.replace(re, '');
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** 取第一个 [model_providers.xxx] / [providers.xxx] 的 slug */
function firstTableSlug(text: string, prefix: 'model_providers' | 'providers'): string {
  const re = new RegExp(`^\\[(${prefix})\\.([^\\]]+)\\]`, 'm');
  const m = text.match(re);
  return m?.[2]?.trim() || 'custom';
}

function looksLikeGrokModelId(model: string): boolean {
  return /^grok[-_]/i.test(model.trim());
}

function kimiModelForWrite(model: string): string {
  const trimmed = model.trim();
  if (!trimmed) return '';
  if (looksLikeGrokModelId(trimmed)) return 'kimi-k2';
  return trimmed;
}

function renameKimiProvider(text: string, from: string, to: string): string {
  if (!from || from === to) return text;
  const fromHeader = `[providers.${from}]`;
  const toHeader = `[providers.${to}]`;
  let next = text.includes(fromHeader)
    ? text.replace(fromHeader, toHeader)
    : text;
  next = next.replace(
    new RegExp(`^(\\s*default_provider\\s*=\\s*)"${escapeRegExp(from)}"`, 'm'),
    `$1"${to}"`,
  );
  next = next.replace(
    new RegExp(`^(\\s*provider\\s*=\\s*)"${escapeRegExp(from)}"`, 'gm'),
    `$1"${to}"`,
  );
  return next;
}

/** 在指定 table 内读写 key（简单行扫描，不引入 TOML 解析器） */
function tomlTableGet(text: string, table: string, key: string): string {
  const header = `[${table}]`;
  const start = text.indexOf(header);
  if (start < 0) return '';
  const after = text.slice(start + header.length);
  const next = after.search(/^\s*\[/m);
  const body = next < 0 ? after : after.slice(0, next);
  return tomlGet(body, key);
}

function tomlTableSet(text: string, table: string, key: string, value: string): string {
  const header = `[${table}]`;
  const start = text.indexOf(header);
  if (start < 0) {
    const pad = text.endsWith('\n') || text.length === 0 ? '' : '\n';
    return `${text}${pad}${header}\n${key} = "${value}"\n`;
  }
  const bodyStart = start + header.length;
  const after = text.slice(bodyStart);
  const nextRel = after.search(/^\s*\[/m);
  const body = nextRel < 0 ? after : after.slice(0, nextRel);
  const rest = nextRel < 0 ? '' : after.slice(nextRel);
  const keyRe = new RegExp(`^(\\s*${escapeRegExp(key)}\\s*=\\s*)"[^"]*"`, 'm');
  let newBody: string;
  if (keyRe.test(body)) {
    newBody = body.replace(keyRe, `$1"${value}"`);
  } else {
    const pad = body.endsWith('\n') || body.length === 0 ? '' : '\n';
    newBody = `${body}${pad}${key} = "${value}"\n`;
  }
  return text.slice(0, bodyStart) + newBody + rest;
}

function grokDefaultAlias(text: string): string {
  const configured = tomlTableGet(text, 'models', 'default');
  if (configured.trim()) return configured.trim();
  const m = text.match(/^\[model\.(?:"([^"]+)"|([^\]]+))\]\s*$/m);
  return (m?.[1] ?? m?.[2] ?? 'grok').trim() || 'grok';
}

function grokModelTable(text: string, alias: string): string {
  const quoted = `model."${alias}"`;
  if (text.includes(`[${quoted}]`)) return quoted;
  const bare = `model.${alias}`;
  if (text.includes(`[${bare}]`)) return bare;
  return quoted;
}

function ensureGrokRegistry(text: string, alias: string): string {
  let out = text;
  if (!/^\s*\[models\]\s*$/m.test(out)) {
    const pad = out.endsWith('\n') || out.length === 0 ? '' : '\n';
    out += `${pad}\n[models]\ndefault = "${alias}"\nweb_search = "${alias}"\n`;
  } else {
    if (!tomlTableGet(out, 'models', 'default')) {
      out = tomlTableSet(out, 'models', 'default', alias);
    }
    if (!tomlTableGet(out, 'models', 'web_search')) {
      out = tomlTableSet(out, 'models', 'web_search', alias);
    }
  }
  const table = grokModelTable(out, alias);
  if (!out.includes(`[${table}]`)) {
    const pad = out.endsWith('\n') || out.length === 0 ? '' : '\n';
    out += `${pad}\n[${table}]\n`;
  }
  return out;
}

function grokHasRegistry(text: string): boolean {
  return /^\s*\[models\]\s*$/m.test(text) || /^\s*\[model\./m.test(text);
}

/** Extra TOML tables (endpoints / auth / …) kept when migrating legacy grok. */
function grokPreservedTables(text: string): string {
  const re = /^\[([^\]]+)\]\s*$/gm;
  const matches = [...text.matchAll(re)];
  let extra = '';
  for (let i = 0; i < matches.length; i++) {
    const header = matches[i]?.[1] ?? '';
    if (/^models$/i.test(header) || /^model\./i.test(header)) continue;
    const start = matches[i]?.index ?? 0;
    const end =
      i + 1 < matches.length ? (matches[i + 1]?.index ?? text.length) : text.length;
    extra += text.slice(start, end);
  }
  return extra.trim();
}

function isOfficialXaiUrl(url: string): boolean {
  return /api\.x\.ai/i.test(url.trim());
}

function grokModelTables(text: string): string[] {
  const tables: string[] = [];
  const re = /^\[model\.(?:"([^"]+)"|([^\]]+))\]\s*$/gm;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text))) {
    const alias = (match[1] ?? match[2] ?? '').trim();
    if (!alias) continue;
    const quoted = `model."${alias}"`;
    const bare = `model.${alias}`;
    const header = text.includes(`[${quoted}]`) ? quoted : bare;
    if (!tables.includes(header)) tables.push(header);
  }
  return tables;
}

function applyGrokFormVars(configText: string, vars: ProviderFormVars): string {
  let text = configText;
  if (!grokHasRegistry(text)) {
    // Migrate the legacy top-level shape while retaining extra tables.
    const legacyModel = tomlGet(configText, 'model');
    const legacyBaseUrl = tomlGet(configText, 'base_url');
    const legacyApiKey = writableSecret(tomlGet(configText, 'api_key'));
    const model = vars.model.trim() || legacyModel || 'grok-4.5';
    const baseUrl = vars.baseUrl.trim() || legacyBaseUrl;
    const extra = grokPreservedTables(configText);
    text = [
      '[models]',
      'default = "grok"',
      'web_search = "grok"',
      '',
      '[model."grok"]',
      `model = "${model}"`,
      ...(baseUrl ? [`base_url = "${baseUrl}"`] : []),
      'env_key = "XAI_API_KEY"',
      'api_backend = "responses"',
      '',
      extra,
    ]
      .filter((line, i, arr) => !(line === '' && arr[i - 1] === ''))
      .join('\n');
    if (legacyApiKey && !text.includes('env_key')) {
      text = tomlTableSet(text, 'model."grok"', 'api_key', legacyApiKey);
    }
    if (text && !text.endsWith('\n')) text += '\n';
  }

  const alias = grokDefaultAlias(text);
  text = ensureGrokRegistry(text, alias);
  const table = grokModelTable(text, alias);
  if (vars.model.trim()) text = tomlTableSet(text, table, 'model', vars.model.trim());
  if (vars.baseUrl.trim()) {
    const existing = tomlTableGet(text, table, 'base_url');
    const next = vars.baseUrl.trim();
    const keepCustom = Boolean(existing) && !isOfficialXaiUrl(existing) && isOfficialXaiUrl(next);
    if (!keepCustom) {
      text = tomlTableSet(text, table, 'base_url', next);
    }
  }
  const hasEnvKey = Boolean(tomlTableGet(text, table, 'env_key'));
  const secret = writableSecret(vars.apiKey);
  if (secret || hasEnvKey) {
    // Prefer env_key; never materialize a live api_key into the document.
    if (!hasEnvKey) {
      text = tomlTableSet(text, table, 'env_key', 'XAI_API_KEY');
    }
    if (tomlTableGet(text, table, 'api_key')) {
      text = tomlTableSet(text, table, 'api_key', REDACTED_MARKER);
    }
  } else if (tomlTableGet(text, table, 'api_key')) {
    // Empty / redacted means keep the native secret on materialize.
    text = tomlTableSet(text, table, 'api_key', REDACTED_MARKER);
  }
  for (const modelTable of grokModelTables(text)) {
    if (!tomlTableGet(text, modelTable, 'api_backend')) {
      text = tomlTableSet(text, modelTable, 'api_backend', 'responses');
    }
  }
  return text;
}

function isOpaqueRedactedToml(configText: string): boolean {
  return configText.trim() === REDACTED_MARKER;
}

/** 从配置原文提取表单变量 */
export function extractFormVars(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
): ProviderFormVars {
  if (format === 'json' || agentId === 'claude') {
    if (configText.trim() === REDACTED_MARKER) {
      return { ...EMPTY_FORM_VARS };
    }
    try {
      const root = JSON.parse(configText || '{}') as Record<string, unknown>;
      if (agentId === 'pi') return extractPiProviderVars(root);
      if (agentId === 'workbuddy') return extractWorkBuddyModelVars(root);
      if (agentId === 'cursor') return extractCursorFormVars(root);
      if (agentId === 'dsh') return extractDshFormVars(root);

      const env =
        root.env && typeof root.env === 'object' && !Array.isArray(root.env)
          ? (root.env as Record<string, unknown>)
          : {};
      const token = firstNonEmptyString(env.ANTHROPIC_AUTH_TOKEN);
      const apiKey = firstNonEmptyString(env.ANTHROPIC_API_KEY, root.apiKey, root.api_key);
      const authEnv: ProviderFormVars['claudeAuthEnv'] = token
        ? 'ANTHROPIC_AUTH_TOKEN'
        : firstNonEmptyString(env.ANTHROPIC_API_KEY)
          ? 'ANTHROPIC_API_KEY'
          : 'ANTHROPIC_AUTH_TOKEN';
      const rawKey = token || apiKey;
      const model = firstNonEmptyString(root.model, env.ANTHROPIC_MODEL);
      return {
        ...EMPTY_FORM_VARS,
        baseUrl: firstNonEmptyString(
          env.ANTHROPIC_BASE_URL,
          root.baseURL,
          root.baseUrl,
          root.base_url,
        ),
        apiKey: sanitizeSecretForForm(rawKey),
        model,
        modelOpus: firstNonEmptyString(env.ANTHROPIC_DEFAULT_OPUS_MODEL),
        modelSonnet: firstNonEmptyString(env.ANTHROPIC_DEFAULT_SONNET_MODEL),
        modelHaiku: firstNonEmptyString(env.ANTHROPIC_DEFAULT_HAIKU_MODEL),
        modelFable: firstNonEmptyString(env.ANTHROPIC_DEFAULT_FABLE_MODEL),
        modelSubagent: firstNonEmptyString(env.CLAUDE_CODE_SUBAGENT_MODEL),
        contextWindow: parseContextWindowChoice(
          firstNonEmptyString(env.CLAUDE_CODE_MAX_CONTEXT_TOKENS, root.contextWindowTokens),
        ),
        claudeAuthEnv: authEnv,
      };
    } catch {
      return { ...EMPTY_FORM_VARS };
    }
  }

  // TOML 全文脱敏时无法解析字段（后端 fail-closed）
  if (isOpaqueRedactedToml(configText)) {
    return { ...EMPTY_FORM_VARS, providerSlug: 'custom' };
  }

  if (agentId === 'codex') {
    // 优先顶层 model_provider，否则第一个 [model_providers.xxx]
    const topSlug = tomlGet(configText, 'model_provider');
    const slug = topSlug || firstTableSlug(configText, 'model_providers');
    const table = `model_providers.${slug}`;
    // API Key 走 settings.auth.OPENAI_API_KEY（见 Provider.authApiKey），不进 TOML
    return {
      ...EMPTY_FORM_VARS,
      model: tomlGet(configText, 'model'),
      baseUrl: tomlTableGet(configText, table, 'base_url'),
      apiKey: '',
      reasoningEffort: tomlGet(configText, 'model_reasoning_effort'),
      wireApi: tomlTableGet(configText, table, 'wire_api') || 'responses',
      providerSlug: slug || 'custom',
    };
  }

  if (agentId === 'kimi') {
    const slug =
      tomlGet(configText, 'default_provider') || firstTableSlug(configText, 'providers');
    const table = `providers.${slug}`;
    const rawKey =
      tomlTableGet(configText, table, 'api_key') || tomlGet(configText, 'api_key');
    const storedModel = tomlGet(configText, 'default_model');
    return {
      ...EMPTY_FORM_VARS,
      model: kimiModelForWrite(storedModel) || storedModel,
      baseUrl: tomlTableGet(configText, table, 'base_url') || tomlGet(configText, 'base_url'),
      apiKey: sanitizeSecretForForm(rawKey),
      providerSlug: slug || 'custom',
    };
  }

  if (agentId === 'grok') {
    const alias = grokDefaultAlias(configText);
    const table = grokModelTable(configText, alias);
    const rawKey =
      tomlTableGet(configText, table, 'api_key') || tomlGet(configText, 'api_key');
    return {
      ...EMPTY_FORM_VARS,
      model:
        tomlTableGet(configText, table, 'model') || tomlGet(configText, 'model'),
      baseUrl:
        tomlTableGet(configText, table, 'base_url') ||
        tomlGet(configText, 'base_url'),
      apiKey: sanitizeSecretForForm(rawKey),
    };
  }

  // 其它顶层 TOML
  return {
    ...EMPTY_FORM_VARS,
    model: tomlGet(configText, 'model'),
    baseUrl: tomlGet(configText, 'base_url'),
    apiKey: sanitizeSecretForForm(tomlGet(configText, 'api_key')),
  };
}

/**
 * 把表单变量写回配置原文。
 * - apiKey 为空：写 `"***"`（JSON 密钥字段）以触发 merge 保留
 * - TOML 全文若本就为 `***` 且无有效编辑：原样返回
 */
export function applyFormVars(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
  vars: ProviderFormVars,
  opts?: { extraEnv?: Record<string, string> },
): string {
  if (format === 'json' || agentId === 'claude') {
    // An opaque redaction marker / empty field starts from a scaffold. Any
    // other malformed or non-object JSON is preserved byte-for-byte so a
    // structured-field edit can never erase the user's original document.
    const trimmed = configText.trim();
    const parsed =
      trimmed === REDACTED_MARKER || !trimmed
        ? { ok: true as const, value: {} }
        : parseJsonObjectConfig(configText);
    if (!parsed.ok) return configText;
    if (agentId === 'pi') return applyPiProviderVars({ ...parsed.value }, vars);
    if (agentId === 'workbuddy') return applyWorkBuddyModelVars({ ...parsed.value }, vars);
    if (agentId === 'cursor') return applyCursorFormVars({ ...parsed.value }, vars);
    if (agentId === 'dsh') return applyDshFormVars({ ...parsed.value }, vars);

    const root: Record<string, unknown> =
      agentId === 'claude'
        ? stripClaudeForeignRootKeys({ ...parsed.value })
        : { ...parsed.value };

    const envRaw = root.env;
    const env: Record<string, unknown> =
      envRaw && typeof envRaw === 'object' && !Array.isArray(envRaw)
        ? { ...(envRaw as Record<string, unknown>) }
        : {};

    if (vars.baseUrl.trim()) env.ANTHROPIC_BASE_URL = vars.baseUrl.trim();
    else delete env.ANTHROPIC_BASE_URL;

    // 只保留选定 auth 字段
    const other =
      vars.claudeAuthEnv === 'ANTHROPIC_AUTH_TOKEN'
        ? 'ANTHROPIC_API_KEY'
        : 'ANTHROPIC_AUTH_TOKEN';
    delete env[other];
    const secret = writableSecret(vars.apiKey);
    if (secret) {
      env[vars.claudeAuthEnv] = secret;
    } else if (env[vars.claudeAuthEnv] != null || configText.includes('ANTHROPIC_')) {
      // 留空 / *** = 保留：回写 redaction marker，不当成新密钥
      env[vars.claudeAuthEnv] = REDACTED_MARKER;
    }

    if (vars.model.trim()) {
      root.model = vars.model.trim();
      env.ANTHROPIC_MODEL = vars.model.trim();
    } else {
      delete root.model;
      delete env.ANTHROPIC_MODEL;
    }

    // 模型位：opus / sonnet / haiku / fable / subagent（自定义 id 均可）
    const roleForm: Record<keyof typeof CLAUDE_MODEL_ROLE_ENV, string> = {
      opus: vars.modelOpus,
      sonnet: vars.modelSonnet,
      haiku: vars.modelHaiku,
      fable: vars.modelFable,
      subagent: vars.modelSubagent,
    };
    for (const [role, envKey] of Object.entries(CLAUDE_MODEL_ROLE_ENV) as [
      keyof typeof CLAUDE_MODEL_ROLE_ENV,
      string,
    ][]) {
      const v = roleForm[role]?.trim() ?? '';
      if (v) env[envKey] = v;
      else delete env[envKey];
    }

    // Claude Code 附加 env（flags 等）；模型位已由表单接管，避免 extraEnv 覆盖清空
    const roleEnvKeys = new Set<string>([
      ...Object.values(CLAUDE_MODEL_ROLE_ENV),
      'ANTHROPIC_MODEL',
    ]);
    const windowTokens = claudeContextWindowFor(
      vars.model,
      contextWindowTokensFromChoice(parseContextWindowChoice(vars.contextWindow)),
    );
    if (windowTokens) {
      env.CLAUDE_CODE_MAX_CONTEXT_TOKENS = String(windowTokens);
      env.CLAUDE_CODE_AUTO_COMPACT_WINDOW = String(windowTokens);
    } else {
      delete env.CLAUDE_CODE_MAX_CONTEXT_TOKENS;
      delete env.CLAUDE_CODE_AUTO_COMPACT_WINDOW;
    }

    if (opts?.extraEnv) {
      for (const [k, v] of Object.entries(opts.extraEnv)) {
        if (
          k === 'CLAUDE_CODE_MAX_CONTEXT_TOKENS' || k === 'CLAUDE_CODE_AUTO_COMPACT_WINDOW'
        ) {
          continue;
        }
        if (roleEnvKeys.has(k)) {
          // 仅当表单对应位为空时才用 extraEnv 填
          const formHas =
            (k === 'ANTHROPIC_MODEL' && vars.model.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.opus && vars.modelOpus.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.sonnet && vars.modelSonnet.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.haiku && vars.modelHaiku.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.fable && vars.modelFable.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.subagent && vars.modelSubagent.trim());
          if (formHas) continue;
        }
        if (v !== undefined && v !== '') env[k] = v;
      }
    }

    root.env = env;
    return JSON.stringify(root, null, 2);
  }

  // TOML 完全脱敏且用户未填任何变量：保持 *** 让后端 merge 整段 content
  const touched =
    vars.baseUrl.trim() ||
    vars.apiKey.trim() ||
    vars.model.trim() ||
    vars.reasoningEffort.trim() ||
    vars.wireApi.trim();
  if (isOpaqueRedactedToml(configText) && !touched) {
    return REDACTED_MARKER;
  }

  // 从脱敏正文开始编辑时，用最小模板起笔
  let text =
    isOpaqueRedactedToml(configText) || !configText.trim()
      ? defaultTomlScaffold(agentId, vars)
      : configText;

  if (agentId === 'codex') {
    const slug = (vars.providerSlug || 'custom').trim() || 'custom';
    const table = `model_providers.${slug}`;
    if (vars.model.trim()) text = tomlSet(text, 'model', vars.model.trim());
    else text = tomlUnset(text, 'model');
    text = tomlSet(text, 'model_provider', slug);
    if (vars.reasoningEffort.trim()) {
      text = tomlSet(text, 'model_reasoning_effort', vars.reasoningEffort.trim());
    }
    // 仅当 name 缺失时补默认；勿覆盖 "Sub2API Grok" 等展示名
    if (!tomlTableGet(text, table, 'name')) {
      text = tomlTableSet(text, table, 'name', slug);
    }
    if (vars.baseUrl.trim()) {
      text = tomlTableSet(text, table, 'base_url', vars.baseUrl.trim());
    }
    if (vars.wireApi.trim()) {
      text = tomlTableSet(text, table, 'wire_api', vars.wireApi.trim());
    } else if (!tomlTableGet(text, table, 'wire_api')) {
      text = tomlTableSet(text, table, 'wire_api', 'responses');
    }
    // Codex API Key → settings_config.auth（Provider.authApiKey），不写进 TOML
    return text;
  }

  if (agentId === 'kimi') {
    const slug = (vars.providerSlug || 'custom').trim() || 'custom';
    const oldSlug =
      tomlGet(text, 'default_provider') || firstTableSlug(text, 'providers') || 'custom';
    if (oldSlug && oldSlug !== slug) {
      text = renameKimiProvider(text, oldSlug, slug);
    }
    const table = `providers.${slug}`;
    const model = kimiModelForWrite(vars.model);
    if (model) text = tomlSet(text, 'default_model', model);
    else text = tomlUnset(text, 'default_model');
    text = tomlSet(text, 'default_provider', slug);
    if (vars.baseUrl.trim()) {
      text = tomlTableSet(text, table, 'base_url', vars.baseUrl.trim());
    }
    const secret = writableSecret(vars.apiKey);
    if (secret) {
      text = tomlTableSet(text, table, 'api_key', secret);
    } else if (tomlTableGet(text, table, 'api_key')) {
      text = tomlTableSet(text, table, 'api_key', REDACTED_MARKER);
    }
    if (model) {
      const modelsTable = `models."${model}"`;
      text = tomlTableSet(text, modelsTable, 'provider', slug);
      text = tomlTableSet(text, modelsTable, 'model', model);
    }
    return text;
  }

  if (agentId === 'grok') return applyGrokFormVars(text, vars);

  // 其它顶层字段
  if (vars.model.trim()) text = tomlSet(text, 'model', vars.model.trim());
  if (vars.baseUrl.trim()) text = tomlSet(text, 'base_url', vars.baseUrl.trim());
  const secret = writableSecret(vars.apiKey);
  if (secret) text = tomlSet(text, 'api_key', secret);
  else if (tomlGet(text, 'api_key')) text = tomlSet(text, 'api_key', REDACTED_MARKER);
  return text;
}

function defaultTomlScaffold(agentId: AgentId, vars: ProviderFormVars): string {
  const slug = (vars.providerSlug || 'custom').trim() || 'custom';
  if (agentId === 'codex') {
    return [
      `model_provider = "${slug}"`,
      `model = "${vars.model.trim() || 'gpt-5.1-codex'}"`,
      'model_reasoning_effort = "high"',
      'disable_response_storage = true',
      'preferred_auth_method = "apikey"',
      `[model_providers.${slug}]`,
      `name = "${slug}"`,
      `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
      'wire_api = "responses"',
      '',
    ].join('\n');
  }
  if (agentId === 'kimi') {
    const model = vars.model.trim() || 'kimi-k2';
    return [
      `default_model = "${model}"`,
      `default_provider = "${slug}"`,
      `[providers.${slug}]`,
      'type = "openai"',
      `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
      'api_key = "sk-xxxxxxxx"',
      `[models."${model}"]`,
      `provider = "${slug}"`,
      `model = "${model}"`,
      'max_context_size = 131072',
      '',
    ].join('\n');
  }
  const grokModel = vars.model.trim() || 'grok-4.5';
  return [
    '[models]',
    'default = "grok"',
    'web_search = "grok"',
    '',
    '[model."grok"]',
    `model = "${grokModel}"`,
    `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
    'env_key = "XAI_API_KEY"',
    'api_backend = "responses"',
    '',
  ].join('\n');
}

/**
 * 弹窗主字段显隐。用户配置场景：默认始终展示 URL / Key / Model，
 * 不再依赖「预设 id」切换显隐。
 */
export function formFieldVisibility(
  agentId: AgentId,
  _presetId = 'custom',
): Record<FormFieldKey, boolean> {
  const isClaude = agentId === 'claude';
  return {
    baseUrl: true,
    apiKey: true,
    model: true,
    modelOpus: isClaude,
    modelSonnet: isClaude,
    modelHaiku: isClaude,
    modelFable: isClaude,
    modelSubagent: isClaude,
    contextWindow: isClaude,
    claudeAuthEnv: isClaude,
    reasoningEffort: agentId === 'codex',
    wireApi: agentId === 'codex',
    providerSlug: agentId === 'pi',
  };
}

/** 字段中文标签 */
export const FORM_FIELD_LABELS: Record<FormFieldKey, string> = {
  baseUrl: '服务地址',
  apiKey: 'API Key',
  model: '主模型',
  modelOpus: 'Opus 模型',
  modelSonnet: 'Sonnet 模型',
  modelHaiku: 'Haiku 模型',
  modelFable: 'Fable 模型',
  modelSubagent: '子任务模型',
  contextWindow: '上下文长度',
  claudeAuthEnv: '密钥写入方式',
  reasoningEffort: '思考强度',
  wireApi: '接口格式',
  providerSlug: '服务商',
};
