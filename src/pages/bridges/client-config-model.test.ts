import { describe, expect, it } from 'vitest';
import type { MessageParams, TranslateFn } from '@/lib/i18n';
import {
  buildClientWriteSpecs,
  canWriteClientConfig,
  clientWriteAgentLabel,
  clientWriteStatusLabel,
  clientWriteTargetSpec,
  clientWriteWireNote,
  defaultClientWriteSelection,
  orderedClientWriteTargets,
  switchWriteLast4,
  type ClientWriteField,
  type ClientWriteSpec,
} from './client-config-model';
import type { CreateRouteTarget } from './create-route-flow';
import type { RouteGraphRow } from './route-graph-model';

const PORT = 43121;
const PENDING_PORT = '{port}';
const LOCAL_TOKEN_LABEL = '令牌（自动生成）';

const STUB_MESSAGES: Record<string, string> = {
  'routes.write.status.applied': 'wrote {name} config',
  'routes.write.status.ready': 'writable',
  'routes.write.status.noUpstream': '{name} endpoint is closed',
  'routes.write.status.hidden': '{name} is hidden',
  'routes.write.status.sourceMissing': 'source login deleted',
  'routes.write.portPending': 'port pending',
  'routes.write.localToken': 'local token',
  'routes.write.fieldLocalAddress': 'local address',
  'routes.write.fieldLocalToken': 'local token field',
  'routes.create.target.claude': 'Claude CLI',
  'routes.create.target.codex': 'Codex CLI',
  'routes.create.target.grok': 'Grok CLI',
  'routes.write.wireNote.claude': 'claude wire note',
  'routes.write.wireNote.codex': 'codex wire note',
  'routes.write.wireNote.grok': 'grok wire note',
};

const stubT: TranslateFn = (key, params) => interpolate(STUB_MESSAGES[key] ?? key, params);

function interpolate(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (all, name: string) => (
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : all
  ));
}

const RESPONSES_ROW: Omit<RouteGraphRow, 'agent'> = {
  localPath: '/v1/responses',
  localEndpointId: 'responses',
  localUrl: `http://127.0.0.1:${PORT}/v1/responses`,
  upstreamBaseUrl: 'https://openrouter.ai/api/v1',
  upstreamPath: '/chat/completions',
  upstreamUrl: 'https://openrouter.ai/api/v1/chat/completions',
  upstreamChannel: 'openai_chat',
  hop: 'convert',
  link: 'solid',
  enabled: true,
  applied: false,
};

const MESSAGES_ROW: Omit<RouteGraphRow, 'agent'> = {
  ...RESPONSES_ROW,
  localPath: '/v1/messages',
  localEndpointId: 'messages',
  localUrl: `http://127.0.0.1:${PORT}/v1/messages`,
};

function row(agent: CreateRouteTarget, overrides: Partial<RouteGraphRow> = {}): RouteGraphRow {
  const base = agent === 'claude' ? MESSAGES_ROW : RESPONSES_ROW;
  return { agent, ...base, ...overrides };
}

function fieldKeys(fields: readonly ClientWriteField[]): string[] {
  return fields.map((field) => field.key);
}

function fieldValue(fields: readonly ClientWriteField[], key: string): string {
  const found = fields.find((field) => field.key === key);
  expect(found, `missing field ${key}`).toBeDefined();
  return found!.value;
}

function specFor(specs: readonly ClientWriteSpec[], agent: CreateRouteTarget): ClientWriteSpec {
  const found = specs.find((spec) => spec.agent === agent);
  expect(found, `missing spec for ${agent}`).toBeDefined();
  return found!;
}

describe('clientWriteTargetSpec', () => {
  it('previews Codex and Grok with distinct config files and a human local address', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('codex'), row('grok')],
      port: PORT,
      sourceMissing: false,
    });
    const codex = specFor(specs, 'codex');
    const grok = specFor(specs, 'grok');

    expect(codex.endpointPath).toBe('/v1/responses');
    expect(grok.endpointPath).toBe('/v1/responses');
    expect(codex.endpointId).toBe('responses');
    expect(grok.endpointId).toBe('responses');

    expect(fieldKeys(codex.fields)).toEqual(['本机地址']);
    expect(fieldKeys(grok.fields)).toEqual(['本机地址']);
    expect(fieldValue(codex.fields, '本机地址')).toBe(`http://127.0.0.1:${PORT}`);
    expect(fieldValue(grok.fields, '本机地址')).toBe(`http://127.0.0.1:${PORT}`);
    expect(fieldKeys(codex.fields).join(' ')).not.toContain('wire_api');
    expect(fieldKeys(grok.fields).join(' ')).not.toContain('api_backend');

    expect(codex.configPath).toBe('~/.codex/config.toml');
    expect(grok.configPath).toBe('~/.grok/config.toml');
    expect(codex.configPath).not.toBe(grok.configPath);
  });

  it('previews Claude settings.json with a local address and placeholder token', () => {
    const spec = clientWriteTargetSpec('claude', { host: '127.0.0.1', port: PORT });

    expect(spec.configPath).toBe('~/.claude/settings.json');
    expect(fieldKeys(spec.fields)).toEqual(['本机地址', '令牌']);

    const baseUrl = spec.fields[0]!.value;
    const token = spec.fields[1]!.value;
    expect(baseUrl).toBe(`http://127.0.0.1:${PORT}`);
    expect(token).toBe(LOCAL_TOKEN_LABEL);
    expect(token.startsWith('sk-')).toBe(false);
    expect(token).not.toMatch(/^ahb_/);
    expect(fieldKeys(spec.fields).join(' ')).not.toContain('ANTHROPIC');
  });

  it('logs switch_write last4 without the full key', () => {
    const full = 'ahb_krTbFixtureLocalTokenValue_dosM';
    expect(switchWriteLast4(full)).toBe('dosM');
    expect(switchWriteLast4(full)).not.toBe(full);
    expect(switchWriteLast4('short')).toBeUndefined();
    expect(switchWriteLast4('')).toBeUndefined();
    expect(switchWriteLast4(null)).toBeUndefined();
  });

  it('previews a local token as last4, never the full value', () => {
    const full = 'ahb_krTbFixtureLocalTokenValue_dosM';
    const spec = clientWriteTargetSpec('claude', {
      host: '127.0.0.1',
      port: PORT,
      localToken: full,
    });
    const token = fieldValue(spec.fields, '令牌');
    expect(token).toBe('末尾 dosM');
    expect(token).not.toContain(full);
    expect(token).not.toMatch(/^ahb_/);
  });

  it('previews Claude model and 1M window when the route pins ox-alpha', () => {
    const spec = clientWriteTargetSpec('claude', {
      host: '127.0.0.1',
      port: PORT,
      model: 'stealth/ox-alpha',
      contextWindowTokens: 1_048_576,
    });
    expect(fieldValue(spec.fields, '主模型')).toBe('stealth/ox-alpha');
    expect(fieldValue(spec.fields, '上下文窗口')).toBe('1M');
  });

  it('uses the same local-address origin for Codex and Grok', () => {
    for (const agent of ['codex', 'grok'] as const) {
      const spec = clientWriteTargetSpec(agent, { host: '127.0.0.1', port: PORT });
      expect(fieldValue(spec.fields, '本机地址')).toBe(`http://127.0.0.1:${PORT}`);
    }
  });

  it('substitutes the real port into every field value when it is known', () => {
    for (const agent of ['claude', 'codex', 'grok'] as const) {
      const spec = clientWriteTargetSpec(agent, { host: '127.0.0.1', port: PORT });
      const urlField = spec.fields[0]!.value;
      expect(urlField).toContain(`:${PORT}`);
      expect(urlField).not.toContain('{port}');
    }
  });

  it('keeps the shared {port} placeholder when the port is unassigned', () => {
    for (const agent of ['claude', 'codex', 'grok'] as const) {
      for (const port of [null, 0]) {
        const spec = clientWriteTargetSpec(agent, { host: '127.0.0.1', port });
        const urlField = spec.fields[0]!.value;
        expect(urlField).toContain(PENDING_PORT);
        expect(urlField).toContain('http://127.0.0.1:');
      }
    }
    const grok = clientWriteTargetSpec('grok', { host: '127.0.0.1', port: null });
    expect(fieldValue(grok.fields, '本机地址')).toBe(`http://127.0.0.1:${PENDING_PORT}`);
  });

  it('uses the translated token label when t is provided', () => {
    const spec = clientWriteTargetSpec('claude', { host: '127.0.0.1', port: null, t: stubT });
    expect(spec.fields[0]!.key).toBe('local address');
    expect(spec.fields[0]!.value).toBe('http://127.0.0.1:{port}');
    expect(spec.fields[1]!.key).toBe('local token field');
    expect(spec.fields[1]!.value).toBe('local token');
  });
});

describe('buildClientWriteSpecs', () => {
  it('emits one spec per row in row order and defaults the host to 127.0.0.1', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('grok'), row('claude'), row('codex')],
      port: PORT,
      sourceMissing: false,
    });

    expect(specs.map((spec) => spec.agent)).toEqual(['grok', 'claude', 'codex']);
    expect(specFor(specs, 'claude').fields[0]!.value).toBe(`http://127.0.0.1:${PORT}`);
  });

  it('honours a custom host and carries the endpoint fields from the row', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('claude', { localUrl: 'http://localhost:5555/v1/messages' })],
      host: 'localhost',
      port: 5555,
      sourceMissing: false,
    });

    const claude = specFor(specs, 'claude');
    expect(claude.fields[0]!.value).toBe('http://localhost:5555');
    expect(claude.endpointPath).toBe('/v1/messages');
    expect(claude.endpointId).toBe('messages');
    expect(claude.endpointUrl).toBe('http://localhost:5555/v1/messages');
  });

  it('normalizes a pending port to a null endpointUrl without inventing a port', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('codex', { localUrl: null })],
      port: null,
      sourceMissing: false,
    });

    const codex = specFor(specs, 'codex');
    expect(codex.endpointUrl).toBeNull();
    expect(codex.fields[0]!.value).toContain(PENDING_PORT);
  });

  it('ranks source_missing over hidden over no_upstream over applied', () => {
    const rows = [
      row('claude', { enabled: false, applied: true }),
      row('codex', { enabled: false, applied: true }),
      row('grok', { enabled: true, applied: true }),
    ];
    const hiddenTargetIds = new Set(['claude', 'codex']);

    const allConditions = buildClientWriteSpecs({
      rows,
      port: PORT,
      sourceMissing: true,
      hiddenTargetIds,
    });
    expect(allConditions.map((spec) => spec.status))
      .toEqual(['source_missing', 'source_missing', 'source_missing']);

    const withoutSource = buildClientWriteSpecs({
      rows,
      port: PORT,
      sourceMissing: false,
      hiddenTargetIds,
    });
    expect(withoutSource.map((spec) => spec.status)).toEqual(['hidden', 'hidden', 'applied']);

    const withoutHidden = buildClientWriteSpecs({
      rows,
      port: PORT,
      sourceMissing: false,
    });
    expect(withoutHidden.map((spec) => spec.status))
      .toEqual(['no_upstream', 'no_upstream', 'applied']);

    const readyRow = buildClientWriteSpecs({
      rows: [row('grok')],
      port: PORT,
      sourceMissing: false,
    });
    expect(readyRow.map((spec) => spec.status)).toEqual(['ready']);
  });

  it('marks only ready and applied rows selectable', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('claude', { enabled: false }), row('codex'), row('grok', { applied: true })],
      port: PORT,
      sourceMissing: false,
      hiddenTargetIds: new Set(['codex']),
    });

    expect(specs.map((spec) => [spec.status, spec.selectable])).toEqual([
      ['no_upstream', false],
      ['hidden', false],
      ['applied', true],
    ]);

    const missing = buildClientWriteSpecs({
      rows: [row('codex')],
      port: PORT,
      sourceMissing: true,
    });
    expect(missing[0]!.status).toBe('source_missing');
    expect(missing[0]!.selectable).toBe(false);

    const ready = buildClientWriteSpecs({
      rows: [row('codex')],
      port: PORT,
      sourceMissing: false,
    });
    expect(ready[0]!.status).toBe('ready');
    expect(ready[0]!.selectable).toBe(true);
  });
});

describe('defaultClientWriteSelection', () => {
  it('pre-ticks only the applied and selectable agents', () => {
    const specs = buildClientWriteSpecs({
      rows: [
        row('claude', { applied: true }),
        row('codex'),
        row('grok', { applied: true, enabled: false }),
      ],
      port: PORT,
      sourceMissing: false,
    });

    expect(defaultClientWriteSelection(specs)).toEqual(['claude']);
  });

  it('returns nothing when the source login is deleted', () => {
    const specs = buildClientWriteSpecs({
      rows: [row('claude', { applied: true }), row('codex', { applied: true })],
      port: PORT,
      sourceMissing: true,
    });

    expect(defaultClientWriteSelection(specs)).toEqual([]);
  });
});

describe('clientWriteStatusLabel', () => {
  it('interpolates {name} into the applied, no_upstream and hidden fallbacks', () => {
    expect(clientWriteStatusLabel('applied', 'Codex')).toBe('已写入 Codex 配置');
    expect(clientWriteStatusLabel('no_upstream', 'Grok')).toBe('这条路由未开放 Grok 端点');
    expect(clientWriteStatusLabel('hidden', 'Claude')).toBe('Claude 已在设置中隐藏');
  });

  it('falls back to Chinese copy for ready and source_missing without t', () => {
    expect(clientWriteStatusLabel('ready', 'Codex')).toBe('可写入');
    expect(clientWriteStatusLabel('source_missing', 'Codex')).toBe('来源登录已删除，无法写入');
  });

  it('uses the translated string when a t is supplied', () => {
    expect(clientWriteStatusLabel('applied', 'Codex', stubT)).toBe('wrote Codex config');
    expect(clientWriteStatusLabel('no_upstream', 'Grok', stubT)).toBe('Grok endpoint is closed');
    expect(clientWriteStatusLabel('hidden', 'Claude', stubT)).toBe('Claude is hidden');
    expect(clientWriteStatusLabel('ready', 'Codex', stubT)).toBe('writable');
    expect(clientWriteStatusLabel('source_missing', 'Codex', stubT)).toBe('source login deleted');
  });
});

describe('clientWriteWireNote', () => {
  it('names the client local config without protocol jargon', () => {
    expect(clientWriteWireNote('claude')).toBe('改 Claude 本机配置');
    expect(clientWriteWireNote('codex')).toBe('改 Codex 本机配置');
    expect(clientWriteWireNote('grok')).toBe('改 Grok 本机配置');
  });

  it('uses the translated note when a t is supplied', () => {
    expect(clientWriteWireNote('codex', stubT)).toBe('codex wire note');
    expect(clientWriteWireNote('grok', stubT)).toBe('grok wire note');
  });
});

describe('clientWriteAgentLabel', () => {
  it('falls back to the product names and honours t', () => {
    expect(clientWriteAgentLabel('claude')).toBe('Claude');
    expect(clientWriteAgentLabel('codex')).toBe('Codex');
    expect(clientWriteAgentLabel('grok')).toBe('Grok');
    expect(clientWriteAgentLabel('grok', stubT)).toBe('Grok CLI');
  });
});

describe('orderedClientWriteTargets', () => {
  it('restores claude → codex → grok order and drops unknown values', () => {
    expect(orderedClientWriteTargets(['grok', 'claude', 'codex']))
      .toEqual(['claude', 'codex', 'grok']);
    expect(orderedClientWriteTargets(['grok', 'kimi' as CreateRouteTarget, 'claude']))
      .toEqual(['claude', 'grok']);
    expect(orderedClientWriteTargets([])).toEqual([]);
  });
});

describe('canWriteClientConfig', () => {
  it('is false for an empty selection and true once an agent is ticked', () => {
    expect(canWriteClientConfig([])).toBe(false);
    expect(canWriteClientConfig(['codex'])).toBe(true);
  });
});
