/**
 * Pure helpers for schema-driven config forms (no React).
 */
import type {
  AgentConfigSchemaDto,
  ConfigFieldSchemaDto,
  ConfigValidationIssueDto,
  ConfigValueTypeDto,
} from '@/lib/backend/contracts/config-types';
import { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';

export type FormValues = Record<string, unknown>;

export function emptyValuesFromSchema(schema: AgentConfigSchemaDto): FormValues {
  const out: FormValues = {};
  for (const f of schema.fields) {
    if (f.default !== undefined && f.default !== null) {
      out[f.key] = f.default;
    } else if (f.valueType.kind === 'boolean') {
      out[f.key] = false;
    } else if (f.valueType.kind === 'number') {
      out[f.key] = '';
    } else {
      out[f.key] = '';
    }
  }
  return out;
}

export function mergeDocumentValues(
  schema: AgentConfigSchemaDto,
  values: Record<string, unknown> | undefined,
): FormValues {
  const base = emptyValuesFromSchema(schema);
  if (!values) return base;
  for (const f of schema.fields) {
    if (f.key in values && values[f.key] !== undefined) {
      base[f.key] = values[f.key];
    }
  }
  return base;
}

/** Secret empty / redacted means "unchanged" on submit. */
export function isSecretUnchanged(value: unknown): boolean {
  if (value == null) return true;
  if (typeof value !== 'string') return false;
  return value === '' || value === SECRET_REDACTED;
}

/** Stored secret is shown as the redacted marker, not an empty field. */
export function isSecretRedacted(value: unknown): boolean {
  return typeof value === 'string' && value === SECRET_REDACTED;
}

export function issuesByField(
  issues: ConfigValidationIssueDto[] | undefined,
): Record<string, string> {
  const map: Record<string, string> = {};
  if (!issues) return map;
  for (const i of issues) {
    if (!map[i.fieldKey]) map[i.fieldKey] = i.message;
  }
  return map;
}

export function isKnownValueType(t: ConfigValueTypeDto): boolean {
  return (
    t.kind === 'string' ||
    t.kind === 'number' ||
    t.kind === 'boolean' ||
    t.kind === 'secret' ||
    t.kind === 'enum'
  );
}

export function fieldControlKind(
  field: ConfigFieldSchemaDto,
): 'string' | 'number' | 'boolean' | 'secret' | 'enum' | 'unsupported' {
  if (!isKnownValueType(field.valueType)) return 'unsupported';
  if (field.secret || field.valueType.kind === 'secret') return 'secret';
  return field.valueType.kind;
}

/** Convert form values to string-map for provider-detect bridge / apply API. */
export function formValuesToStringRecord(values: FormValues): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(values)) {
    if (v == null) {
      out[k] = '';
    } else if (typeof v === 'boolean' || typeof v === 'number') {
      out[k] = String(v);
    } else {
      out[k] = String(v);
    }
  }
  return out;
}
