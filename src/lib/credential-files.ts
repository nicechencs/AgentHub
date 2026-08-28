/**
 * Associated login files for an authorization (filename + redacted snapshot).
 * Paths are display paths (`~/...`); opening expands them in the file manager.
 */
import type { AccountKind, AgentId, Provider } from '@/lib/types';

export interface CredentialFileView {
  /** Basename, e.g. `auth.json`. */
  name: string;
  /** Redacted file text. Never contains a usable secret. */
  content: string;
}

export type AgentLivePathSet = {
  config: string;
  auth?: string | null;
  extra?: string[];
  openDir: string;
};

const SECRET_KEY =
  /^(api[_-]?key|apikey|token|auth[_-]?token|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?token|authorization|password|client[_-]?secret|private[_-]?key)$/i;

const SECRET_PREFIX = /^(sk-|xai-|ghp_|gho_|github_pat_|xox[bp]-)/i;

export function authFileName(agentId: string): string {
  if (agentId === 'claude') return '.credentials.json';
  if (agentId === 'kimi') return 'kimi-code.json';
  if (agentId === 'dsh') return '.credentials.yaml';
  return 'auth.json';
}

export function configFileName(agentId: string): string {
  if (agentId === 'claude' || agentId === 'pi' || agentId === 'workbuddy') {
    return 'settings.json';
  }
  if (agentId === 'dsh') return 'cordis.patch.yml';
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
    files.push({ name, content: stringifyRedacted(value) });
  };
  const pushText = (name: string, text: string) => {
    if (seen.has(name)) return;
    seen.add(name);
    files.push({ name, content: redactFreeformText(text) });
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
      return [{ name, content: stringifyRedacted(parsed) }];
    }
  }
  return [{ name, content: redactFreeformText(text) }];
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
  const out: Record<string, unknown> = {};
  if (format) out.format = format;
  for (const key of ['env_key', 'provider', 'email', 'api_key']) {
    if (credentials[key] !== undefined) out[key] = credentials[key];
  }
  if (Object.keys(out).length === 0) {
    return format ? { format } : {};
  }
  return out;
}

function stringifyRedacted(value: unknown): string {
  return `${JSON.stringify(redactValue(value), null, 2)}\n`;
}

function redactValue(value: unknown): unknown {
  if (typeof value === 'string') {
    return redactSecretString(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactValue(item));
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      out[key] = SECRET_KEY.test(key) && typeof child === 'string'
        ? redactSecretString(child, true)
        : redactValue(child);
    }
    return out;
  }
  return value;
}

function redactSecretString(value: string, force = false): string {
  const trimmed = value.trim();
  if (!trimmed || trimmed === '***') return trimmed || value;
  if (force) return '***';
  if (SECRET_PREFIX.test(trimmed) && trimmed.length >= 12) return '***';
  return value;
}

function redactFreeformText(text: string): string {
  return text.replace(
    /^(\s*(?:api[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token|token|password|client[_-]?secret)\s*[=:]\s*)(["']?)([^\s"',;#]+)\2/gim,
    (_match, prefix: string, quote: string) => `${prefix}${quote}***${quote}`,
  );
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
