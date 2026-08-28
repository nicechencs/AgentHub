/**
 * Backend configuration DTOs (mirror core platform/config).
 * Secrets use SECRET_REDACTED; apply with "***" or empty keeps native secret.
 */

export const SECRET_REDACTED = '***';

export type ConfigValueTypeDto =
  | { kind: 'string' }
  | { kind: 'number' }
  | { kind: 'boolean' }
  | { kind: 'secret' }
  | { kind: 'enum'; options: string[] };

export interface FieldValidationDto {
  minLength?: number | null;
  maxLength?: number | null;
  pattern?: string | null;
}

export interface ConfigFieldSchemaDto {
  key: string;
  label: string;
  valueType: ConfigValueTypeDto;
  required?: boolean;
  secret?: boolean;
  default?: unknown;
  validation?: FieldValidationDto | null;
  help?: string | null;
  capability?: string | null;
  /** Outside-native secret location, e.g. `auth.OPENAI_API_KEY`. */
  secretStorage?: string | null;
}

export type NativeConfigFormatDto = 'json' | 'toml';

export interface AgentConfigSchemaDto {
  agentKey: string;
  schemaVersion: number;
  nativeFormat: NativeConfigFormatDto;
  relativePath: string;
  fields: ConfigFieldSchemaDto[];
}

export interface NormalizedConfigDocumentDto {
  agentKey: string;
  schemaVersion: number;
  values: Record<string, unknown>;
  unknownNative: unknown;
  path?: string | null;
  missing?: boolean;
}

export interface ConfigValidationIssueDto {
  fieldKey: string;
  code: string;
  message: string;
}

export interface ConfigValidationResultDto {
  ok: boolean;
  issues: ConfigValidationIssueDto[];
}

export interface FieldChangeDto {
  fieldKey: string;
  from?: unknown;
  to?: unknown;
  secret?: boolean;
}

export interface ConfigChangePlanDto {
  agentKey: string;
  schemaVersion: number;
  targetPath: string;
  fieldChanges: FieldChangeDto[];
}

export interface ConfigApplyResultDto {
  document: NormalizedConfigDocumentDto;
  plan: ConfigChangePlanDto;
}

export interface ConfigPort {
  getAgentConfigSchema(agentId: string): Promise<AgentConfigSchemaDto>;
  readAgentConfig(agentId: string): Promise<NormalizedConfigDocumentDto>;
  validateAgentConfig(
    agentId: string,
    values: Record<string, unknown>,
  ): Promise<ConfigValidationResultDto>;
  planAgentConfig(
    agentId: string,
    values: Record<string, unknown>,
  ): Promise<ConfigChangePlanDto>;
  applyAgentConfig(
    agentId: string,
    values: Record<string, unknown>,
  ): Promise<ConfigApplyResultDto>;
  /**
   * Build provider-pool settings_config from schema field values (no live FS write).
   * `baseRaw` is the existing settings_config object when editing.
   */
  materializeAgentConfig(
    agentId: string,
    values: Record<string, unknown>,
    baseRaw?: unknown,
  ): Promise<unknown>;
}
