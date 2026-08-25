/** Pure view-model for the per-Agent client-config write dialog. No React, no IO. */
import type { MessageKey, MessageParams, TranslateFn } from '@/lib/i18n';
import {
  ROUTE_ENDPOINT_HOST,
  ROUTE_ENDPOINT_PENDING_PORT,
  type RouteEndpointId,
} from '@/lib/route-endpoints';
import {
  formatClaudeContextWindow,
} from '@/lib/claude-client-env';
import { CREATE_ROUTE_TARGETS, type CreateRouteTarget } from './create-route-flow';
import type { RouteGraphRow } from './route-graph-model';

export type ClientWriteStatus =
  | 'applied'
  | 'ready'
  | 'no_upstream'
  | 'hidden'
  | 'source_missing';

export type ClientWriteField = { key: string; value: string };

export type ClientWriteSpec = {
  agent: CreateRouteTarget;
  /** Config file AgentHub rewrites, e.g. `~/.codex/config.toml`. */
  configPath: string;
  /** Ordered key/value pairs written into that file. Secrets are never real values. */
  fields: ClientWriteField[];
  /** Loopback path this agent will call. */
  endpointPath: string;
  endpointId: RouteEndpointId;
  /** Full loopback URL, or null while the port is pending. */
  endpointUrl: string | null;
  status: ClientWriteStatus;
  /** Whether the row's checkbox can be ticked. */
  selectable: boolean;
};

const CLAUDE_CONFIG_PATH = '~/.claude/settings.json';
const CODEX_CONFIG_PATH = '~/.codex/config.toml';
const GROK_CONFIG_PATH = '~/.grok/config.toml';

const WRITE_COPY = {
  portPending: '端口分配中，写入后生效',
  localToken: '本机令牌（自动生成）',
  statusApplied: '已写入 {name} 配置',
  statusReady: '可写入',
  statusNoUpstream: '这条路由未开放 {name} 端点',
  statusHidden: '{name} 已在设置中隐藏',
  statusSourceMissing: '来源登录已删除，无法写入',
  fieldLocalAddress: '本机地址',
  fieldLocalToken: '本机令牌',
  fieldModel: '主模型',
  fieldContextWindow: '上下文窗口',
  wireNoteClaude: '改 Claude 本机配置',
  wireNoteCodex: '改 Codex 本机配置',
  wireNoteGrok: '改 Grok 本机配置',
} as const;

function applyParams(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (all, name: string) => (
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : all
  ));
}

function writeText(
  t: TranslateFn | undefined,
  key: string,
  fallback: string,
  params?: MessageParams,
): string {
  if (!t) return applyParams(fallback, params);
  return t(key as MessageKey, params);
}

function normalizePort(port: number | null | undefined): number | null {
  return typeof port === 'number' && port > 0 ? port : null;
}

function normalizeHost(host: string | undefined): string {
  return host?.trim() || ROUTE_ENDPOINT_HOST;
}

/**
 * Loopback origin for the dialog. A pending port keeps the shared `{port}`
 * placeholder so the preview matches RouteEndpointUrl elsewhere on the page;
 * the dialog explains it once with `routes.write.portPending`.
 */
function writeOrigin(host: string, port: number | null): string {
  return `http://${host}:${port ?? ROUTE_ENDPOINT_PENDING_PORT}`;
}

function localAddressField(origin: string, t?: TranslateFn): ClientWriteField {
  return {
    key: writeText(t, 'routes.write.fieldLocalAddress', WRITE_COPY.fieldLocalAddress),
    value: origin,
  };
}

function localTokenField(t?: TranslateFn): ClientWriteField {
  return {
    key: writeText(t, 'routes.write.fieldLocalToken', WRITE_COPY.fieldLocalToken),
    value: writeText(t, 'routes.write.localToken', WRITE_COPY.localToken),
  };
}

/** Config file + preview fields for one agent on a local bridge. Port-independent shape. */
export function clientWriteTargetSpec(
  agent: CreateRouteTarget,
  input: {
    host: string;
    port: number | null;
    t?: TranslateFn;
    model?: string;
    contextWindowTokens?: number | null;
  },
): { configPath: string; fields: ClientWriteField[] } {
  const origin = writeOrigin(normalizeHost(input.host), normalizePort(input.port));
  if (agent === 'claude') {
    const fields = [localAddressField(origin, input.t), localTokenField(input.t)];
    const model = input.model?.trim() ?? '';
    if (model) {
      fields.push({
        key: writeText(input.t, 'routes.write.fieldModel', WRITE_COPY.fieldModel),
        value: model,
      });
    }
    const windowLabel = formatClaudeContextWindow(input.contextWindowTokens);
    if (windowLabel) {
      fields.push({
        key: writeText(input.t, 'routes.write.fieldContextWindow', WRITE_COPY.fieldContextWindow),
        value: windowLabel,
      });
    }
    return {
      configPath: CLAUDE_CONFIG_PATH,
      fields,
    };
  }
  if (agent === 'grok') {
    return {
      configPath: GROK_CONFIG_PATH,
      fields: [localAddressField(origin, input.t)],
    };
  }
  return {
    configPath: CODEX_CONFIG_PATH,
    fields: [localAddressField(origin, input.t)],
  };
}

function resolveClientWriteStatus(input: {
  sourceMissing: boolean;
  hidden: boolean;
  noUpstream: boolean;
  applied: boolean;
}): ClientWriteStatus {
  if (input.sourceMissing) return 'source_missing';
  if (input.hidden) return 'hidden';
  if (input.noUpstream) return 'no_upstream';
  if (input.applied) return 'applied';
  return 'ready';
}

export function buildClientWriteSpecs(input: {
  rows: readonly RouteGraphRow[];
  host?: string;
  port?: number | null;
  sourceMissing: boolean;
  hiddenTargetIds?: ReadonlySet<string>;
  listedModels?: readonly string[];
  contextWindowTokens?: number | null;
  t?: TranslateFn;
}): ClientWriteSpec[] {
  const host = normalizeHost(input.host);
  const port = normalizePort(input.port);
  const hiddenTargetIds = input.hiddenTargetIds ?? new Set<string>();
  const model = input.listedModels?.find((item) => item.trim())?.trim() ?? '';
  return input.rows.map((row) => {
    const target = clientWriteTargetSpec(row.agent, {
      host,
      port,
      t: input.t,
      model,
      contextWindowTokens: input.contextWindowTokens,
    });
    const status = resolveClientWriteStatus({
      sourceMissing: input.sourceMissing,
      hidden: hiddenTargetIds.has(row.agent),
      noUpstream: row.enabled === false,
      applied: row.applied,
    });
    return {
      agent: row.agent,
      configPath: target.configPath,
      fields: target.fields,
      endpointPath: row.localPath,
      endpointId: row.localEndpointId,
      endpointUrl: row.localUrl?.trim() ? row.localUrl : null,
      status,
      selectable: status === 'applied' || status === 'ready',
    };
  });
}

/** Pre-tick what is already live so re-writing is an idempotent refresh. */
export function defaultClientWriteSelection(
  specs: readonly ClientWriteSpec[],
): CreateRouteTarget[] {
  return specs
    .filter((spec) => spec.selectable && spec.status === 'applied')
    .map((spec) => spec.agent);
}

export function clientWriteStatusLabel(
  status: ClientWriteStatus,
  agentLabel: string,
  t?: TranslateFn,
): string {
  if (status === 'applied') {
    return writeText(t, 'routes.write.status.applied', WRITE_COPY.statusApplied, {
      name: agentLabel,
    });
  }
  if (status === 'no_upstream') {
    return writeText(t, 'routes.write.status.noUpstream', WRITE_COPY.statusNoUpstream, {
      name: agentLabel,
    });
  }
  if (status === 'hidden') {
    return writeText(t, 'routes.write.status.hidden', WRITE_COPY.statusHidden, {
      name: agentLabel,
    });
  }
  if (status === 'source_missing') {
    return writeText(t, 'routes.write.status.sourceMissing', WRITE_COPY.statusSourceMissing);
  }
  return writeText(t, 'routes.write.status.ready', WRITE_COPY.statusReady);
}

/** Beginner-facing note: which client's local config this write updates. */
export function clientWriteWireNote(agent: CreateRouteTarget, t?: TranslateFn): string {
  if (agent === 'claude') {
    return writeText(t, 'routes.write.wireNote.claude', WRITE_COPY.wireNoteClaude);
  }
  if (agent === 'grok') {
    return writeText(t, 'routes.write.wireNote.grok', WRITE_COPY.wireNoteGrok);
  }
  return writeText(t, 'routes.write.wireNote.codex', WRITE_COPY.wireNoteCodex);
}

export function clientWriteAgentLabel(agent: CreateRouteTarget, t?: TranslateFn): string {
  if (agent === 'claude') return writeText(t, 'routes.create.target.claude', 'Claude');
  if (agent === 'grok') return writeText(t, 'routes.create.target.grok', 'Grok');
  return writeText(t, 'routes.create.target.codex', 'Codex');
}

export function canWriteClientConfig(selected: readonly CreateRouteTarget[]): boolean {
  return selected.length > 0;
}

/** Narrow a selection back to the canonical CREATE_ROUTE_TARGETS order. */
export function orderedClientWriteTargets(
  selected: readonly CreateRouteTarget[],
): CreateRouteTarget[] {
  return CREATE_ROUTE_TARGETS.filter((target) => selected.includes(target));
}
