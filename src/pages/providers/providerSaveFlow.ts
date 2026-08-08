/**
 * ProviderEditDialog save / schema-gate helpers (pure, unit-testable).
 * Connection lifecycle stays on ConnectionService; this only gates
 * Configuration projector vs legacy applyFormVars for the provider pool.
 */
import type { AgentCatalogStatus } from '@/app/runtime/agent-catalog-store';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import type {
  ConfigValidationIssueDto,
  ConfigValidationResultDto,
} from '@/lib/backend/contracts/config-types';
import type { AgentId, Provider } from '@/lib/types';
import type { ProviderFormVars } from '@/lib/provider-detect';
import { REDACTED_MARKER } from '@/lib/provider-detect';

/** UI schema load state for the Provider edit dialog. */
export type SchemaUiStatus = 'idle' | 'loading' | 'ready' | 'unsupported' | 'error';

/**
 * Catalog-driven projector expectation.
 * - required: configSchemaVersion is a number → must use schema/validate/materialize
 * - unsupported: configSchemaVersion is explicitly null → legacy applyFormVars OK
 * - unknown: catalog not ready / entry missing / version undefined → fail closed
 */
export type ProjectorExpectation =
  | { kind: 'required'; version: number }
  | { kind: 'unsupported' }
  | { kind: 'unknown'; reason: string };

export function resolveProjectorExpectation(args: {
  catalogStatus: AgentCatalogStatus;
  entry: AgentCatalogEntryDto | null | undefined;
}): ProjectorExpectation {
  const { catalogStatus, entry } = args;

  if (catalogStatus === 'idle' || catalogStatus === 'loading') {
    return { kind: 'unknown', reason: 'catalog_not_ready' };
  }
  if (catalogStatus === 'error' || catalogStatus === 'unavailable') {
    return { kind: 'unknown', reason: 'catalog_unavailable' };
  }
  // ready
  if (!entry) {
    return { kind: 'unknown', reason: 'entry_missing' };
  }
  // Distinguish undefined (capability unknown) from explicit null (no projector).
  if (entry.configSchemaVersion === undefined) {
    return { kind: 'unknown', reason: 'version_undefined' };
  }
  if (entry.configSchemaVersion === null) {
    return { kind: 'unsupported' };
  }
  if (typeof entry.configSchemaVersion === 'number') {
    return { kind: 'required', version: entry.configSchemaVersion };
  }
  return { kind: 'unknown', reason: 'version_invalid' };
}

/** Save is only allowed once schema path is known: ready or unsupported. */
export function canSaveWithSchemaStatus(status: SchemaUiStatus): boolean {
  return status === 'ready' || status === 'unsupported';
}

export type SchemaLoadPlan =
  | { action: 'wait' }
  | { action: 'unsupported' }
  | { action: 'error'; message: string }
  | { action: 'load_schema' };

/**
 * Decide what the dialog should do when opening / retrying schema load.
 * Does not call the network — pure plan from catalog expectation.
 */
export function planSchemaLoad(expectation: ProjectorExpectation): SchemaLoadPlan {
  switch (expectation.kind) {
    case 'unknown':
      if (expectation.reason === 'catalog_not_ready') {
        return { action: 'wait' };
      }
      return {
        action: 'error',
        message: schemaErrorMessage(expectation.reason),
      };
    case 'unsupported':
      return { action: 'unsupported' };
    case 'required':
      return { action: 'load_schema' };
  }
}

export function schemaErrorMessage(reason: string): string {
  switch (reason) {
    case 'catalog_not_ready':
      return 'Agent Catalog 尚未就绪，无法确认配置能力';
    case 'catalog_unavailable':
      return 'Agent Catalog 不可用，无法确认配置能力';
    case 'entry_missing':
      return '当前 Agent 不在 Catalog 中，无法确认配置能力';
    case 'version_undefined':
      return 'Catalog 未声明 configSchemaVersion，禁止保存';
    case 'version_invalid':
      return 'Catalog 的 configSchemaVersion 无效';
    case 'schema_load_failed':
      return '加载配置 schema 失败';
    default:
      return reason || '配置能力未知';
  }
}

export type ProviderSavePath = 'projector' | 'legacy' | 'blocked';

export function resolveSavePath(schemaStatus: SchemaUiStatus): ProviderSavePath {
  if (schemaStatus === 'ready') return 'projector';
  if (schemaStatus === 'unsupported') return 'legacy';
  return 'blocked';
}

export type ProviderSaveFailureCode =
  | 'schema_not_ready'
  | 'invalid_json'
  | 'validation_failed'
  | 'materialize_failed'
  | 'upsert_failed';

export type ProviderSaveResult =
  | { ok: true; provider: Provider; path: 'projector' | 'legacy' }
  | {
      ok: false;
      code: ProviderSaveFailureCode;
      message: string;
      issues?: ConfigValidationIssueDto[];
      /** True when form/config text must be preserved (always for these failures). */
      preserveInput: true;
    };

export interface ProviderSaveFlowInput {
  agentId: AgentId;
  schemaStatus: SchemaUiStatus;
  isEdit: boolean;
  existing?: Provider | null;
  name: string;
  useOfficial: boolean;
  officialLabel?: string;
  officialPresetId?: string;
  configText: string;
  configFormat: 'json' | 'toml';
  vars: ProviderFormVars;
  saveVars: ProviderFormVars;
  finalFormat: 'json' | 'toml';
  /** Scaffold / official base when configText is empty or redacted. */
  baseText: string;
}

export interface ProviderSaveFlowDeps {
  validateAgentConfig: (
    agentId: string,
    values: Record<string, unknown>,
  ) => Promise<ConfigValidationResultDto>;
  materializeAgentConfig: (
    agentId: string,
    values: Record<string, unknown>,
    baseRaw?: unknown,
  ) => Promise<unknown>;
  applyFormVars: (
    agentId: AgentId,
    configText: string,
    format: 'json' | 'toml',
    vars: ProviderFormVars,
  ) => string;
  upsertProvider: (p: Provider) => Promise<Provider>;
}

/**
 * Parse JSON base for materialize. Fail closed — never substitute {}.
 */
export function parseJsonConfigBase(baseText: string):
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; message: string } {
  const trimmed = baseText.trim();
  if (!trimmed) {
    return { ok: true, value: {} };
  }
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

function buildTomlBaseRaw(
  agentId: AgentId,
  baseText: string,
  authApiKey: string | undefined,
): Record<string, unknown> {
  const raw: Record<string, unknown> = {
    format: 'toml',
    content: baseText,
  };
  if (
    agentId === 'codex' &&
    typeof authApiKey === 'string' &&
    authApiKey &&
    authApiKey !== REDACTED_MARKER
  ) {
    raw.auth = { OPENAI_API_KEY: authApiKey };
  }
  return raw;
}

function materializeToConfigText(
  agentId: AgentId,
  raw: unknown,
):
  | { ok: true; finalText: string; authApiKey?: string }
  | { ok: false; message: string } {
  if (raw == null) {
    return { ok: false, message: 'materialize 返回空结果' };
  }
  if (typeof raw !== 'object') {
    return { ok: false, message: 'materialize 返回无法识别的结果' };
  }
  const obj = raw as {
    format?: string;
    content?: string;
    auth?: { OPENAI_API_KEY?: string };
  };
  if (obj.format === 'toml' && typeof obj.content === 'string') {
    return {
      ok: true,
      finalText: obj.content,
      authApiKey:
        agentId === 'codex' && obj.auth?.OPENAI_API_KEY
          ? obj.auth.OPENAI_API_KEY
          : undefined,
    };
  }
  // JSON object document
  try {
    return { ok: true, finalText: JSON.stringify(raw, null, 2) };
  } catch (e) {
    return {
      ok: false,
      message: e instanceof Error ? e.message : '无法序列化 materialize 结果',
    };
  }
}

function resolveAuthApiKeyInput(
  agentId: AgentId,
  isEdit: boolean,
  vars: ProviderFormVars,
  existing?: Provider | null,
): string | undefined {
  if (agentId === 'codex') {
    if (vars.apiKey.trim()) return vars.apiKey.trim();
    if (isEdit) return '';
    return undefined;
  }
  return existing?.authApiKey;
}

function buildDisplayName(input: ProviderSaveFlowInput): string {
  const { name, useOfficial, officialLabel, vars } = input;
  if (name.trim()) return name.trim();
  if (useOfficial && officialLabel) return officialLabel;
  if (vars.baseUrl) {
    try {
      return new URL(vars.baseUrl).host;
    } catch {
      return 'API Key';
    }
  }
  return 'API Key';
}

function buildProviderDraft(
  input: ProviderSaveFlowInput,
  finalText: string,
  authApiKey: string | undefined,
): Provider {
  const { agentId, isEdit, existing, useOfficial, officialPresetId, finalFormat } = input;
  const displayName = buildDisplayName(input);
  const preset =
    useOfficial && officialPresetId
      ? officialPresetId
      : isEdit && existing
        ? existing.preset || 'custom'
        : 'custom';

  if (isEdit && existing) {
    return {
      ...existing,
      name: displayName,
      preset,
      configText: finalText,
      configFormat: finalFormat,
      authApiKey,
      official: useOfficial,
    };
  }
  return {
    id: `p-${Date.now()}`,
    agentId,
    name: displayName,
    preset,
    configText: finalText,
    configFormat: finalFormat,
    authApiKey,
    isCurrent: false,
    official: useOfficial,
  };
}

/**
 * Full save orchestration for ProviderEditDialog.
 * Projector path: parse → validate → materialize → upsert.
 * Legacy path: applyFormVars → upsert (only when schemaStatus === unsupported).
 * Any failure returns before upsert (except upsert itself).
 */
export async function runProviderSaveFlow(
  input: ProviderSaveFlowInput,
  deps: ProviderSaveFlowDeps,
): Promise<ProviderSaveResult> {
  const path = resolveSavePath(input.schemaStatus);
  if (path === 'blocked') {
    return {
      ok: false,
      code: 'schema_not_ready',
      message: '配置 schema 未就绪，禁止保存',
      preserveInput: true,
    };
  }

  let finalText: string;
  let authApiKey = resolveAuthApiKeyInput(
    input.agentId,
    input.isEdit,
    input.vars,
    input.existing,
  );

  if (path === 'projector') {
    // Build baseRaw without falling back to {} on parse errors.
    let baseRaw: unknown;
    if (input.finalFormat === 'toml') {
      baseRaw = buildTomlBaseRaw(input.agentId, input.baseText, authApiKey);
    } else {
      const parsed = parseJsonConfigBase(input.baseText);
      if (!parsed.ok) {
        return {
          ok: false,
          code: 'invalid_json',
          message: parsed.message,
          preserveInput: true,
        };
      }
      baseRaw = parsed.value;
    }

    const values = { ...input.saveVars } as unknown as Record<string, unknown>;

    let validation: ConfigValidationResultDto;
    try {
      validation = await deps.validateAgentConfig(input.agentId, values);
    } catch (e) {
      return {
        ok: false,
        code: 'validation_failed',
        message: e instanceof Error ? e.message : String(e),
        preserveInput: true,
      };
    }
    if (!validation.ok) {
      const summary =
        validation.issues.map((i) => i.message).filter(Boolean).join('；') ||
        '配置校验未通过';
      return {
        ok: false,
        code: 'validation_failed',
        message: summary,
        issues: validation.issues,
        preserveInput: true,
      };
    }

    let raw: unknown;
    try {
      raw = await deps.materializeAgentConfig(input.agentId, values, baseRaw);
    } catch (e) {
      return {
        ok: false,
        code: 'materialize_failed',
        message: e instanceof Error ? e.message : String(e),
        preserveInput: true,
      };
    }

    const materialized = materializeToConfigText(input.agentId, raw);
    if (!materialized.ok) {
      return {
        ok: false,
        code: 'materialize_failed',
        message: materialized.message,
        preserveInput: true,
      };
    }
    finalText = materialized.finalText;
    if (materialized.authApiKey !== undefined) {
      authApiKey = materialized.authApiKey;
    }
  } else {
    // legacy — only when Catalog explicitly has configSchemaVersion: null
    finalText = deps.applyFormVars(
      input.agentId,
      input.baseText,
      input.finalFormat,
      input.saveVars,
    );
  }

  const draft = buildProviderDraft(input, finalText, authApiKey);
  try {
    const provider = await deps.upsertProvider(draft);
    return { ok: true, provider, path };
  } catch (e) {
    return {
      ok: false,
      code: 'upsert_failed',
      message: e instanceof Error ? e.message : String(e),
      preserveInput: true,
    };
  }
}
