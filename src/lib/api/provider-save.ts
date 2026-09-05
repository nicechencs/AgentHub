/**
 * Provider save use case: projector validate/materialize vs legacy applyFormVars.
 * Connection lifecycle stays on ConnectionService.
 */
import type { AgentConfigSchemaDto, ConfigValidationIssueDto, ConfigValidationResultDto } from '@/lib/backend/contracts/config-types';
import type { AgentKey, Provider } from '@/lib/types';
import type { ProviderFormVars } from '@/lib/provider-detect';
import { piProviderSlotById } from '@/lib/pi-provider-slots';
import { REDACTED_MARKER } from '@/lib/provider-detect';
import type { LiveOccupancyDto } from '@/lib/backend/contracts/agent-catalog-types';
import { isListOccupancy } from '@/lib/backend/contracts/agent-catalog-types';

/** UI schema load state for the Provider edit dialog. */
export type SchemaUiStatus = 'idle' | 'loading' | 'ready' | 'unsupported' | 'error';

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
  agentId: AgentKey;
  schemaStatus: SchemaUiStatus;
  /** Schema returned by the backend; required on the projector path. */
  configSchema?: AgentConfigSchemaDto | null;
  isEdit: boolean;
  existing?: Provider | null;
  /** Stable id for new records. Auto-generated when omitted. */
  id?: string;
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
  /** Catalog occupancy; list-occupancy agents write live on add. */
  occupancy?: LiveOccupancyDto | null;
}

/**
 * Project the shared provider form onto the backend's declared field set.
 *
 * ProviderFormVars is intentionally a union-shaped UI model shared by
 * Claude/Codex/Kimi/Grok. The backend validators are strict and must only see
 * keys declared by the active agent schema. Empty strings and redaction
 * markers are preserved because they carry submit semantics for secrets and
 * optional fields.
 */
export function projectValuesToSchema(
  schema: AgentConfigSchemaDto,
  source: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of schema.fields) {
    if (Object.prototype.hasOwnProperty.call(source, field.key)) {
      const value = source[field.key];
      if (value !== undefined) out[field.key] = value;
    }
  }
  return out;
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
    agentId: AgentKey,
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

export const AUTH_OPENAI_API_KEY_STORAGE = 'auth.OPENAI_API_KEY';

export function schemaUsesAuthEnvelope(
  schema?: AgentConfigSchemaDto | null,
): boolean {
  return (schema?.fields ?? []).some(
    (field) => field.secretStorage === AUTH_OPENAI_API_KEY_STORAGE,
  );
}

function usesAuthEnvelope(
  agentId: AgentKey,
  schema?: AgentConfigSchemaDto | null,
): boolean {
  if (schemaUsesAuthEnvelope(schema)) return true;
  return !schema && agentId === 'codex';
}

function buildTomlBaseRaw(
  agentId: AgentKey,
  schema: AgentConfigSchemaDto | null | undefined,
  baseText: string,
  authApiKey: string | undefined,
): Record<string, unknown> {
  const raw: Record<string, unknown> = {
    format: 'toml',
    content: baseText,
  };
  if (
    usesAuthEnvelope(agentId, schema) &&
    typeof authApiKey === 'string' &&
    authApiKey &&
    authApiKey !== REDACTED_MARKER
  ) {
    raw.auth = { OPENAI_API_KEY: authApiKey };
  }
  return raw;
}

function materializeToConfigText(
  agentId: AgentKey,
  schema: AgentConfigSchemaDto | null | undefined,
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
        usesAuthEnvelope(agentId, schema) &&
        obj.auth?.OPENAI_API_KEY &&
        obj.auth.OPENAI_API_KEY !== REDACTED_MARKER
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
  agentId: AgentKey,
  schema: AgentConfigSchemaDto | null | undefined,
  isEdit: boolean,
  vars: ProviderFormVars,
  existing?: Provider | null,
): string | undefined {
  if (usesAuthEnvelope(agentId, schema)) {
    if (vars.apiKey.trim()) return vars.apiKey.trim();
    if (isEdit) return '';
    return undefined;
  }
  return existing?.authApiKey;
}

function buildDisplayName(input: ProviderSaveFlowInput): string {
  const { name, useOfficial, officialLabel, vars, agentId } = input;
  if (name.trim()) return name.trim();
  if (useOfficial && officialLabel) return officialLabel;
  if (agentId === 'pi') {
    const slot = piProviderSlotById(vars.providerSlug);
    if (slot && slot.id !== 'custom') return slot.label;
  }
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
    id: input.id ?? `p-${Date.now()}`,
    agentId,
    name: displayName,
    preset,
    configText: finalText,
    configFormat: finalFormat,
    authApiKey,
    isCurrent: isListOccupancy(input.occupancy),
    official: useOfficial,
  };
}

/**
 * Full save orchestration for a provider pool entry.
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
    input.configSchema,
    input.isEdit,
    input.vars,
    input.existing,
  );

  if (path === 'projector') {
    if (!input.configSchema) {
      return {
        ok: false,
        code: 'schema_not_ready',
        message: '配置 schema 未就绪，禁止保存',
        preserveInput: true,
      };
    }
    // Build baseRaw without falling back to {} on parse errors.
    let baseRaw: unknown;
    if (input.finalFormat === 'toml') {
      baseRaw = buildTomlBaseRaw(
        input.agentId,
        input.configSchema,
        input.baseText,
        authApiKey,
      );
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

    const values = projectValuesToSchema(
      input.configSchema,
      input.saveVars as unknown as Record<string, unknown>,
    );

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

    const materialized = materializeToConfigText(
      input.agentId,
      input.configSchema,
      raw,
    );
    if (!materialized.ok) {
      return {
        ok: false,
        code: 'materialize_failed',
        message: materialized.message,
        preserveInput: true,
      };
    }
    finalText = materialized.finalText;
    if (
      materialized.authApiKey !== undefined &&
      materialized.authApiKey !== REDACTED_MARKER
    ) {
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
