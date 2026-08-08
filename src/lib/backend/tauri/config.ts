import type { ConfigPort } from '@/lib/backend/contracts/config-types';
import type {
  AgentConfigSchemaDto,
  ConfigApplyResultDto,
  ConfigChangePlanDto,
  ConfigValidationResultDto,
  NormalizedConfigDocumentDto,
} from '@/lib/backend/contracts/config-types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:config');

export function createTauriConfigPort(): ConfigPort {
  return {
    async getAgentConfigSchema(agentId) {
      try {
        return await invoke<AgentConfigSchemaDto>('get_agent_config_schema', {
          agentId,
        });
      } catch (e) {
        log.error('get_agent_config_schema failed', e);
        throw e;
      }
    },
    async readAgentConfig(agentId) {
      try {
        return await invoke<NormalizedConfigDocumentDto>('read_agent_config', {
          agentId,
        });
      } catch (e) {
        log.error('read_agent_config failed', e);
        throw e;
      }
    },
    async validateAgentConfig(agentId, values) {
      try {
        return await invoke<ConfigValidationResultDto>('validate_agent_config', {
          agentId,
          values,
        });
      } catch (e) {
        log.error('validate_agent_config failed', e);
        throw e;
      }
    },
    async planAgentConfig(agentId, values) {
      try {
        return await invoke<ConfigChangePlanDto>('plan_agent_config', {
          agentId,
          values,
        });
      } catch (e) {
        log.error('plan_agent_config failed', e);
        throw e;
      }
    },
    async applyAgentConfig(agentId, values) {
      try {
        return await invoke<ConfigApplyResultDto>('apply_agent_config', {
          agentId,
          values,
        });
      } catch (e) {
        log.error('apply_agent_config failed', e);
        throw e;
      }
    },
    async materializeAgentConfig(agentId, values, baseRaw) {
      try {
        return await invoke<unknown>('materialize_agent_config', {
          agentId,
          values,
          baseRaw: baseRaw ?? null,
        });
      } catch (e) {
        log.error('materialize_agent_config failed', e);
        throw e;
      }
    },
  };
}
