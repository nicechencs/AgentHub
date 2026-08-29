/**
 * Catalog/schema UI gate for the provider edit dialog.
 * Save orchestration lives in `@/lib/api/provider-save`.
 */
import type { AgentCatalogStatus } from '@/app/runtime/agent-catalog-store';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import type { SchemaUiStatus } from '@/lib/api/provider-save';

export type { SchemaUiStatus };

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

/**
 * Dialog save gate. Model is never required. Custom connections may omit a
 * model; official mode still locks the official model via `withDefaultModel`.
 * Fetch status does not participate.
 */
export function canSaveProviderForm(args: {
  schemaStatus: SchemaUiStatus;
  configError: string | null;
  isEdit: boolean;
  apiKey: string;
  piNeedsUrl?: boolean;
  baseUrl?: string;
  /** Ignored; model is optional. */
  model?: string;
}): boolean {
  return (
    canSaveWithSchemaStatus(args.schemaStatus) &&
    !args.configError &&
    (args.isEdit ? true : Boolean(args.apiKey.trim())) &&
    (!args.piNeedsUrl || Boolean((args.baseUrl ?? '').trim()))
  );
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
      return '连接能力尚未就绪，请稍后再试';
    case 'catalog_unavailable':
      return '暂时无法确认该 Agent 的配置能力';
    case 'entry_missing':
      return '当前 Agent 暂不支持此配置操作';
    case 'version_undefined':
      return '该 Agent 的配置能力未声明，禁止保存';
    case 'version_invalid':
      return '该 Agent 的配置版本无效，请更新应用后重试';
    case 'schema_load_failed':
      return '加载配置表单失败';
    default:
      return reason || '配置能力未知';
  }
}
