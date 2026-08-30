/**
 * Associated login files for an authorization (filename + file snapshot).
 * Paths are display paths (`~/...`); opening expands them in the file manager.
 * Preview text is not masked by field name — this is the user's own local file.
 */
import type { AccountKind, AgentId, Provider } from '@/lib/types';

export interface CredentialFileView {
  /** Basename, e.g. `auth.json`. */
  name: string;
  /** File snapshot as stored. Not masked in this layer. */
  content: string;
}

export type AgentLivePathSet = {
  config: string;
  auth?: string | null;
  extra?: string[];
  openDir: string;
};

export function authFileName(agentId: string): string {
  if (agentId === 'claude') return '.credentials.json';
  if (agentId === 'kimi') return 'kimi-code.json';
  if (agentId === 'dsh') return '.credentials.yaml';
  if (agentId === 'zcode') return 'config.json';
  return 'auth.json';
}

export function configFileName(agentId: string): string {
  if (agentId === 'claude' || agentId === 'pi') {
    return 'settings.json';
  }
  // WorkBuddy API Key rows live in models.json; settings.json is sandbox/plugins.
  if (agentId === 'workbuddy') return 'models.json';
  if (agentId === 'dsh') return 'cordis.patch.yml';
  if (agentId === 'zcode') return 'config.json';
  return 'config.toml';
}

export function defaultLivePathForFile(agentId: string, fileName: string): string {
  const known: Record<string, Record<string, string>> = {
    grok: {
      'auth.json': '~/.grok/auth.json',
      'config.toml': '~/.grok/config.toml',
    },
    codex: {
      'auth.json': '~/.codex/auth.json',
      'config.toml': '~/.codex/config.toml',
    },
    claude: {
      '.credentials.json': '~/.claude/.credentials.json',
      'settings.json': '~/.claude/settings.json',
      '.claude.json': '~/.claude.json',
    },
    kimi: {
      'kimi-code.json': '~/.kimi-code/credentials/kimi-code.json',
      'config.toml': '~/.kimi-code/config.toml',
    },
    pi: {
      'auth.json': '~/.pi/agent/auth.json',
      'settings.json': '~/.pi/agent/settings.json',
      'models.json': '~/.pi/agent/models.json',
    },
    workbuddy: {
      'settings.json': '~/.workbuddy/settings.json',
      'models.json': '~/.workbuddy/models.json',
      '.mcp.json': '~/.workbuddy/.mcp.json',
    },
    dsh: {
      '.credentials.yaml': '~/.dsh/.credentials.yaml',
      'cordis.patch.yml': '~/.dsh/cordis.patch.yml',
    },
    zcode: {
      'config.json': '~/.zcode/v2/config.json',
      'cli/config.json': '~/.zcode/cli/config.json',
    },
    cursor: {
      'auth.json': '~/.cursor/auth.json',
    },
  };
  const mapped = known[agentId]?.[fileName];
  if (mapped) return mapped;
  if (fileName.includes('/')) return `~/.${agentId}/${fileName}`;
  return `~/.${agentId}/${fileName}`;
}

export function resolveCredentialFilePath(
  fileName: string,
  live: AgentLivePathSet | null | undefined,
  agentId: string,
): string {
  const candidates = [live?.auth, live?.config, ...(live?.extra ?? [])]
    .map((value) => (typeof value === 'string' ? stripDisplayNotes(value) : ''))
    .filter((value) => isLikelyFsPath(value));
  const match = candidates.find((path) => pathEndsWithFile(path, fileName));
  if (match) return match;
  const dir = live?.openDir ? stripDisplayNotes(live.openDir) : '';
  if (isLikelyFsPath(dir)) {
    if (fileName === 'kimi-code.json') return `${trimSlash(dir)}/credentials/kimi-code.json`;
    return `${trimSlash(dir)}/${fileName}`;
  }
  return defaultLivePathForFile(agentId, fileName);
}

export function extractAccountCredentialFiles(input: {
  agentId: AgentId | string;
  kind: AccountKind;
  credentials?: Record<string, unknown>;
  source?: string;
  format?: string;
}): CredentialFileView[] {
  const credentials = input.credentials ?? {};
  const format =
    input.format
    ?? (typeof credentials.format === 'string' ? credentials.format : undefined);
  const files: CredentialFileView[] = [];
  const seen = new Set<string>();

  const pushJson = (name: string, value: unknown) => {
    if (seen.has(name)) return;
    seen.add(name);
    files.push({ name, content: stringifyPreview(value) });
  };
  const pushText = (name: string, text: string) => {
    if (seen.has(name)) return;
    seen.add(name);
    files.push({ name, content: text });
  };

  const authName = fileNameFromSource(input.source, 'auth') ?? authFileName(input.agentId);
  const configName = fileNameFromSource(input.source, 'config') ?? configFileName(input.agentId);

  const body = credentials.body;
  if (body && typeof body === 'object') {
    pushJson(authName, body);
  }
  const auth = credentials.auth;
  if (auth && typeof auth === 'object' && !seen.has(authName)) {
    pushJson(authName, auth);
  }
  const credentialsFile = credentials.credentials_file;
  if (credentialsFile && typeof credentialsFile === 'object') {
    pushJson('kimi-code.json', credentialsFile);
  }

  const content =
    (typeof credentials.content === 'string' ? credentials.content : undefined)
    ?? (typeof credentials.config === 'string' ? credentials.config : undefined);
  if (content && content.trim()) {
    const looksJson = looksLikeJson(content) || configName.endsWith('.json');
    if (looksJson) {
      const parsed = tryParseJson(content);
      if (parsed !== undefined) pushJson(configName, parsed);
      else pushText(configName, content);
    } else {
      pushText(configName, content);
    }
  }

  const catalog = catalogFileSnapshot(input.agentId, credentials);
  if (catalog && !seen.has(catalog.name)) {
    pushJson(catalog.name, catalog.value);
  }

  if (files.length === 0) {
    const snapshot = remainingCredentialSnapshot(credentials, format);
    const name = input.kind === 'oauth' || isAuthFormat(format)
      ? authName
      : configName;
    pushJson(name, snapshot);
  }

  return files;
}

export function extractProviderCredentialFiles(provider: Pick<
  Provider,
  'agentId' | 'configText' | 'configFormat'
>): CredentialFileView[] {
  const name = configFileName(provider.agentId);
  const text = provider.configText?.trim() ?? '';
  if (!text) {
    return [{ name, content: provider.configFormat === 'json' ? '{}' : '' }];
  }
  if (provider.configFormat === 'json' || looksLikeJson(text)) {
    const parsed = tryParseJson(text);
    if (parsed !== undefined) {
      return [{ name, content: stringifyPreview(parsed) }];
    }
  }
  return [{ name, content: text }];
}

function isAuthFormat(format: string | undefined): boolean {
  return format === 'auth_json' || format === 'credentials_json' || format === 'oauth';
}

function fileNameFromSource(
  source: string | undefined,
  kind: 'auth' | 'config',
): string | undefined {
  if (!source) return undefined;
  const trimmed = source.trim();
  if (!trimmed || trimmed.includes('+') || trimmed === 'manual' || trimmed === 'live') {
    return undefined;
  }
  const base = trimmed.replace(/\\/g, '/').split('/').pop() ?? trimmed;
  if (!/\.(json|toml|ya?ml)$/i.test(base)) return undefined;
  const isAuth = /auth|credential/i.test(base);
  if (kind === 'auth' && isAuth) return base;
  if (kind === 'config' && !isAuth) return base;
  return undefined;
}

function remainingCredentialSnapshot(
  credentials: Record<string, unknown>,
  format: string | undefined,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...credentials };
  if (format && out.format === undefined) out.format = format;
  return out;
}

function pickCredentialString(
  credentials: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = credentials[key];
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

/**
 * Catalog-append agents store many API Key rows in one JSON file.
 * Preview the native row, not a Hub `{ format, provider, api_key }` stub.
 */
function catalogFileSnapshot(
  agentId: AgentId | string,
  credentials: Record<string, unknown>,
): { name: string; value: unknown } | undefined {
  if (agentId === 'zcode') {
    const value = zcodeCatalogFileSnapshot(credentials);
    return value === undefined ? undefined : { name: 'config.json', value };
  }
  if (agentId === 'workbuddy') {
    const value = workbuddyCatalogFileSnapshot(credentials);
    return value === undefined ? undefined : { name: 'models.json', value };
  }
  return undefined;
}

/**
 * Rebuild the on-disk `config.json` shape for one ZCode catalog row.
 * Hub stores flattened `api_key` / `base_url` fields; the file preview should
 * look like `provider.<id>` from `~/.zcode/v2/config.json`.
 */
function zcodeCatalogFileSnapshot(
  credentials: Record<string, unknown>,
): Record<string, unknown> | undefined {
  const id = pickCredentialString(credentials, 'provider_id');
  const name = pickCredentialString(credentials, 'provider_name');
  const base =
    pickCredentialString(credentials, 'base_url')
    ?? pickCredentialString(credentials, 'baseURL');
  const catalog = credentials.catalog_row;
  if (catalog && typeof catalog === 'object' && !Array.isArray(catalog)) {
    return { provider: { [id ?? 'custom']: catalog } };
  }
  if (!id && !name && !base) return undefined;

  const row: Record<string, unknown> = {};
  if (name) row.name = name;
  const kind = pickCredentialString(credentials, 'kind');
  if (kind) row.kind = kind;
  const options: Record<string, unknown> = {};
  if (credentials.api_key !== undefined) options.apiKey = credentials.api_key;
  if (base) options.baseURL = base;
  if (Object.keys(options).length > 0) row.options = options;
  if (credentials.models !== undefined) row.models = credentials.models;
  return { provider: { [id ?? 'custom']: row } };
}

/**
 * Rebuild one WorkBuddy `models.json` entry (file is a top-level array).
 */
function workbuddyCatalogFileSnapshot(
  credentials: Record<string, unknown>,
): unknown[] | undefined {
  const catalog = credentials.catalog_row;
  if (catalog && typeof catalog === 'object' && !Array.isArray(catalog)) {
    return [catalog];
  }
  const id =
    pickCredentialString(credentials, 'model_id')
    ?? pickCredentialString(credentials, 'id');
  const name = pickCredentialString(credentials, 'name');
  const url =
    pickCredentialString(credentials, 'url')
    ?? pickCredentialString(credentials, 'base_url')
    ?? pickCredentialString(credentials, 'baseURL');
  if (!id && !name && !url) return undefined;

  const row: Record<string, unknown> = {};
  if (id) row.id = id;
  if (name) row.name = name;
  const vendor = pickCredentialString(credentials, 'vendor');
  if (vendor) row.vendor = vendor;
  if (url) row.url = url;
  if (credentials.api_key !== undefined) row.apiKey = credentials.api_key;
  for (const flag of ['supportsToolCall', 'supportsImages', 'supportsReasoning', 'useCustomProtocol', 'reasoning']) {
    if (credentials[flag] !== undefined) row[flag] = credentials[flag];
  }
  return [row];
}

function stringifyPreview(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function looksLikeJson(text: string): boolean {
  const trimmed = text.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

function tryParseJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function isLikelyFsPath(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  return trimmed.startsWith('~') || trimmed.startsWith('/') || /^[A-Za-z]:[\\/]/.test(trimmed);
}

function stripDisplayNotes(path: string): string {
  return path
    .replace(/（[^）]*）/g, '')
    .replace(/\s*\([^)]*\)\s*$/g, '')
    .trim();
}

function pathEndsWithFile(path: string, fileName: string): boolean {
  const normalized = path.replace(/\\/g, '/');
  const name = fileName.replace(/\\/g, '/');
  const base = name.split('/').pop() ?? name;
  return normalized === name
    || normalized.endsWith(`/${name}`)
    || normalized.endsWith(`/${base}`);
}

function trimSlash(path: string): string {
  return path.replace(/[/\\]+$/, '');
}
