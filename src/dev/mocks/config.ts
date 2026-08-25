/**
 * Browser mock config port — fixtures only, no real secrets.
 */
import type {
  AgentConfigSchemaDto,
  ConfigPort,
  NormalizedConfigDocumentDto,
} from '@/lib/backend/contracts/config-types';
import { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';
import {
  claudeContextWindowFor,
  contextWindowTokensFromChoice,
  parseContextWindowChoice,
} from '@/lib/claude-client-env';

const SCHEMAS: Record<string, AgentConfigSchemaDto> = {
  claude: {
    agentKey: 'claude',
    schemaVersion: 1,
    nativeFormat: 'json',
    relativePath: 'settings.json',
    fields: [
      { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
      {
        key: 'apiKey',
        label: 'API Key / Auth Token',
        valueType: { kind: 'secret' },
        secret: true,
      },
      {
        key: 'claudeAuthEnv',
        label: 'Auth env name',
        valueType: {
          kind: 'enum',
          options: ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'],
        },
      },
      { key: 'model', label: 'Model', valueType: { kind: 'string' } },
      { key: 'modelOpus', label: 'Opus model', valueType: { kind: 'string' } },
      { key: 'modelSonnet', label: 'Sonnet model', valueType: { kind: 'string' } },
      { key: 'modelHaiku', label: 'Haiku model', valueType: { kind: 'string' } },
      { key: 'modelFable', label: 'Fable model', valueType: { kind: 'string' } },
      { key: 'modelSubagent', label: 'Subagent model', valueType: { kind: 'string' } },
      {
        key: 'contextWindow',
        label: 'Context window',
        valueType: { kind: 'enum', options: ['auto', '200000', '1048576'] },
      },
    ],
  },
  codex: {
    agentKey: 'codex',
    schemaVersion: 1,
    nativeFormat: 'toml',
    relativePath: 'config.toml',
    fields: [
      { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
      {
        key: 'apiKey',
        label: 'OpenAI API Key',
        valueType: { kind: 'secret' },
        secret: true,
      },
      { key: 'model', label: 'Model', valueType: { kind: 'string' } },
      { key: 'reasoningEffort', label: 'Reasoning effort', valueType: { kind: 'string' } },
      { key: 'wireApi', label: 'Wire API', valueType: { kind: 'string' } },
      { key: 'providerSlug', label: 'Provider slug', valueType: { kind: 'string' } },
    ],
  },
  kimi: {
    agentKey: 'kimi',
    schemaVersion: 1,
    nativeFormat: 'toml',
    relativePath: 'config.toml',
    fields: [
      { key: 'model', label: 'Default model', valueType: { kind: 'string' } },
      { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
      { key: 'apiKey', label: 'API Key', valueType: { kind: 'secret' }, secret: true },
      { key: 'providerSlug', label: 'Provider slug', valueType: { kind: 'string' } },
    ],
  },
  grok: {
    agentKey: 'grok',
    schemaVersion: 2,
    nativeFormat: 'toml',
    relativePath: 'config.toml',
    fields: [
      { key: 'model', label: 'Model', valueType: { kind: 'string' } },
      { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
      { key: 'apiKey', label: 'API Key', valueType: { kind: 'secret' }, secret: true },
    ],
  },
  dsh: {
    agentKey: 'dsh',
    schemaVersion: 1,
    nativeFormat: 'json',
    relativePath: 'cordis.patch.yml',
    fields: [
      { key: 'provider', label: 'Provider', valueType: { kind: 'string' } },
      { key: 'model', label: 'Model', valueType: { kind: 'string' } },
      { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
      {
        key: 'thinking',
        label: 'Thinking',
        valueType: { kind: 'enum', options: ['enabled', 'disabled'] },
      },
      {
        key: 'reasoningEffort',
        label: 'Reasoning effort',
        valueType: { kind: 'enum', options: ['off', 'low', 'high', 'max'] },
      },
      { key: 'maxTokens', label: 'Max tokens', valueType: { kind: 'number' } },
      { key: 'apiKeyEnv', label: 'API key env name', valueType: { kind: 'string' } },
      { key: 'apiKey', label: 'API Key', valueType: { kind: 'secret' }, secret: true },
    ],
  },
};

let mockValues: Record<string, Record<string, unknown>> = {
  claude: {
    baseUrl: 'https://api.anthropic.com',
    apiKey: SECRET_REDACTED,
    claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
    model: 'claude-sonnet',
    modelOpus: '',
    modelSonnet: '',
    modelHaiku: '',
    modelFable: '',
    modelSubagent: '',
    contextWindow: 'auto',
  },
};

export function resetMockConfig() {
  mockValues = {
    claude: {
      baseUrl: 'https://api.anthropic.com',
      apiKey: SECRET_REDACTED,
      claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
      model: 'claude-sonnet',
      modelOpus: '',
      modelSonnet: '',
      modelHaiku: '',
      modelFable: '',
      modelSubagent: '',
      contextWindow: 'auto',
    },
  };
}

export function createMockConfigPort(): ConfigPort {
  return {
    async getAgentConfigSchema(agentId) {
      const schema = SCHEMAS[agentId];
      if (!schema) {
        throw new Error(`unsupported config projector for ${agentId} [unsupported]`);
      }
      return schema;
    },
    async readAgentConfig(agentId) {
      const schema = SCHEMAS[agentId];
      if (!schema) {
        throw new Error(`unsupported config projector for ${agentId} [unsupported]`);
      }
      const values = mockValues[agentId] ?? {
        ...Object.fromEntries(schema.fields.map((f) => [f.key, f.secret ? SECRET_REDACTED : ''])),
      };
      const doc: NormalizedConfigDocumentDto = {
        agentKey: agentId,
        schemaVersion: schema.schemaVersion,
        values,
        unknownNative: { format: schema.nativeFormat, content: '' },
        missing: false,
        path: schema.relativePath,
      };
      return doc;
    },
    async validateAgentConfig(agentId, values) {
      const schema = SCHEMAS[agentId];
      if (!schema) {
        throw new Error(`unsupported config projector for ${agentId} [unsupported]`);
      }
      const known = new Set(schema.fields.map((field) => field.key));
      const unknown = Object.keys(values).filter((key) => !known.has(key));
      if (unknown.length > 0) {
        return {
          ok: false,
          issues: unknown.map((fieldKey) => ({
            fieldKey,
            code: 'unknown_field',
            message: `unknown field: ${fieldKey}`,
          })),
        };
      }
      return { ok: true, issues: [] };
    },
    async planAgentConfig(agentId, values) {
      return {
        agentKey: agentId,
        schemaVersion: SCHEMAS[agentId]?.schemaVersion ?? 1,
        targetPath: SCHEMAS[agentId]?.relativePath ?? 'config',
        fieldChanges: Object.keys(values).map((fieldKey) => ({
          fieldKey,
          to: values[fieldKey],
          secret: fieldKey === 'apiKey',
        })),
      };
    },
    async applyAgentConfig(agentId, values) {
      const prev = mockValues[agentId] ?? {};
      const next = { ...prev, ...values };
      if (
        values.apiKey === SECRET_REDACTED ||
        values.apiKey === '' ||
        values.apiKey == null
      ) {
        next.apiKey = (prev.apiKey as string) || SECRET_REDACTED;
      } else {
        next.apiKey = SECRET_REDACTED; // never keep plaintext in mock store display
      }
      mockValues[agentId] = next;
      const plan = await this.planAgentConfig(agentId, values);
      const document = await this.readAgentConfig(agentId);
      return { document, plan };
    },
    async materializeAgentConfig(agentId, values, baseRaw) {
      const schema = SCHEMAS[agentId];
      if (!schema) {
        throw new Error(`unsupported config projector for ${agentId} [unsupported]`);
      }
      if (schema.nativeFormat === 'json') {
        const base =
          baseRaw && typeof baseRaw === 'object' && !Array.isArray(baseRaw)
            ? { ...(baseRaw as Record<string, unknown>) }
            : {};
        const env =
          base.env && typeof base.env === 'object' && !Array.isArray(base.env)
            ? { ...(base.env as Record<string, unknown>) }
            : {};
        if (typeof values.baseUrl === 'string' && values.baseUrl) {
          env.ANTHROPIC_BASE_URL = values.baseUrl;
        }
        if (typeof values.model === 'string' && values.model) {
          base.model = values.model;
          env.ANTHROPIC_MODEL = values.model;
        }
        if (typeof values.contextWindow === 'string' || typeof values.model === 'string') {
          const model = typeof values.model === 'string' ? values.model : String(base.model ?? '');
          const override = contextWindowTokensFromChoice(
            parseContextWindowChoice(
              typeof values.contextWindow === 'string'
                ? values.contextWindow
                : String(env.CLAUDE_CODE_MAX_CONTEXT_TOKENS ?? ''),
            ),
          );
          const windowTokens = claudeContextWindowFor(model, override);
          if (windowTokens) {
            env.CLAUDE_CODE_MAX_CONTEXT_TOKENS = String(windowTokens);
            env.CLAUDE_CODE_AUTO_COMPACT_WINDOW = String(windowTokens);
          } else {
            delete env.CLAUDE_CODE_MAX_CONTEXT_TOKENS;
            delete env.CLAUDE_CODE_AUTO_COMPACT_WINDOW;
          }
        }
        if (
          typeof values.apiKey === 'string' &&
          values.apiKey &&
          values.apiKey !== SECRET_REDACTED
        ) {
          const authEnv =
            typeof values.claudeAuthEnv === 'string'
              ? values.claudeAuthEnv
              : 'ANTHROPIC_AUTH_TOKEN';
          env[authEnv] = values.apiKey;
        }
        base.env = env;
        return base;
      }
      // TOML dual-shape for pool
      return {
        format: 'toml',
        content:
          typeof (baseRaw as { content?: string } | null)?.content === 'string'
            ? (baseRaw as { content: string }).content
            : `model = "${String(values.model ?? '')}"\n`,
        ...(typeof values.apiKey === 'string' &&
        values.apiKey &&
        values.apiKey !== SECRET_REDACTED
          ? { auth: { OPENAI_API_KEY: values.apiKey } }
          : {}),
      };
    },
  };
}
