/**
 * Configuration API façade — delegates to app runtime backend.config.
 */
import { getBackend } from '@/app/runtime';
import type {
  AgentConfigSchemaDto,
  ConfigApplyResultDto,
  ConfigChangePlanDto,
  ConfigValidationResultDto,
  NormalizedConfigDocumentDto,
} from '@/lib/backend/contracts/config-types';

export type {
  AgentConfigSchemaDto,
  ConfigApplyResultDto,
  ConfigChangePlanDto,
  ConfigValidationResultDto,
  NormalizedConfigDocumentDto,
} from '@/lib/backend/contracts/config-types';
export { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';

export async function getAgentConfigSchema(agentId: string): Promise<AgentConfigSchemaDto> {
  return getBackend().config.getAgentConfigSchema(agentId);
}

export async function readAgentConfig(agentId: string): Promise<NormalizedConfigDocumentDto> {
  return getBackend().config.readAgentConfig(agentId);
}

export async function validateAgentConfig(
  agentId: string,
  values: Record<string, unknown>,
): Promise<ConfigValidationResultDto> {
  return getBackend().config.validateAgentConfig(agentId, values);
}

export async function planAgentConfig(
  agentId: string,
  values: Record<string, unknown>,
): Promise<ConfigChangePlanDto> {
  return getBackend().config.planAgentConfig(agentId, values);
}

export async function applyAgentConfig(
  agentId: string,
  values: Record<string, unknown>,
): Promise<ConfigApplyResultDto> {
  return getBackend().config.applyAgentConfig(agentId, values);
}

export async function materializeAgentConfig(
  agentId: string,
  values: Record<string, unknown>,
  baseRaw?: unknown,
): Promise<unknown> {
  return getBackend().config.materializeAgentConfig(agentId, values, baseRaw);
}
