/**
 * R02: ProviderEditDialog Configuration fail-closed — pure flow tests.
 * Vitest runs in node (no jsdom); exercises real branch order via provider-save.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { MOCK_AGENT_CATALOG } from '@/dev/mocks/fixtures/agent-catalog';
import { createMockConfigPort } from '@/dev/mocks/config';
import {
  SECRET_REDACTED,
  type AgentConfigSchemaDto,
} from '@/lib/backend/contracts/config-types';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import type { Provider } from '@/lib/types';
import {
  applyFormVars,
  EMPTY_FORM_VARS,
  REDACTED_MARKER,
  withDefaultModel,
  type ProviderFormVars,
} from '@/lib/provider-detect';
import {
  parseJsonConfigBase,
  projectValuesToSchema,
  resolveSavePath,
  runProviderSaveFlow,
  type ProviderSaveFlowDeps,
  type ProviderSaveFlowInput,
  type SchemaUiStatus,
} from '@/lib/api/provider-save';
import {
  canSaveProviderForm,
  canSaveWithSchemaStatus,
  planSchemaLoad,
  resolveProjectorExpectation,
} from '@/pages/providers/provider-schema-gate';
import { createTranslator } from '@/lib/i18n';
import { getConfigTextError } from './ProviderEditDialog';

const TEST_CLAUDE_SCHEMA: AgentConfigSchemaDto = {
  agentKey: 'claude',
  schemaVersion: 1,
  nativeFormat: 'json',
  relativePath: 'settings.json',
  fields: [
    { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
    { key: 'apiKey', label: 'API Key', valueType: { kind: 'secret' }, secret: true },
    {
      key: 'claudeAuthEnv',
      label: 'Auth env',
      valueType: { kind: 'enum', options: ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'] },
    },
    { key: 'model', label: 'Model', valueType: { kind: 'string' } },
    { key: 'modelOpus', label: 'Opus', valueType: { kind: 'string' } },
    { key: 'modelSonnet', label: 'Sonnet', valueType: { kind: 'string' } },
    { key: 'modelHaiku', label: 'Haiku', valueType: { kind: 'string' } },
    { key: 'modelFable', label: 'Fable', valueType: { kind: 'string' } },
    { key: 'modelSubagent', label: 'Subagent', valueType: { kind: 'string' } },
    {
      key: 'contextWindow',
      label: 'Context window',
      valueType: { kind: 'enum', options: ['auto', '200000', '1048576'] },
    },
  ],
};

function entry(
  key: string,
  configSchemaVersion?: number | null,
): AgentCatalogEntryDto {
  const base: AgentCatalogEntryDto = {
    key,
    displayName: key,
    integrationVersion: 1,
    capabilities: {},
    installChannels: [],
  };
  if (configSchemaVersion === undefined && arguments.length < 2) {
    // omit field → undefined capability
    return base;
  }
  return { ...base, configSchemaVersion };
}

function baseInput(
  overrides: Partial<ProviderSaveFlowInput> = {},
): ProviderSaveFlowInput {
  const vars: ProviderFormVars = {
    ...EMPTY_FORM_VARS,
    baseUrl: 'https://api.example.com',
    apiKey: 'sk-test-key',
    model: 'model-x',
  };
  return {
    agentId: 'claude',
    schemaStatus: 'ready',
    configSchema: TEST_CLAUDE_SCHEMA,
    isEdit: false,
    existing: null,
    name: 'Test Provider',
    useOfficial: false,
    configText: '{"env":{}}',
    configFormat: 'json',
    vars,
    saveVars: vars,
    finalFormat: 'json',
    baseText: '{"env":{}}',
    ...overrides,
  };
}

function mockDeps(
  overrides: Partial<ProviderSaveFlowDeps> = {},
): ProviderSaveFlowDeps & {
  validateAgentConfig: ReturnType<typeof vi.fn>;
  materializeAgentConfig: ReturnType<typeof vi.fn>;
  applyFormVars: ReturnType<typeof vi.fn>;
  upsertProvider: ReturnType<typeof vi.fn>;
} {
  const validateAgentConfig = vi.fn(async () => ({ ok: true, issues: [] }));
  const materializeAgentConfig = vi.fn(async (_id, values) => ({
    env: { ANTHROPIC_AUTH_TOKEN: values.apiKey },
    model: values.model,
  }));
  const applyFormVars = vi.fn(
    (_id: string, text: string, _fmt: string, vars: ProviderFormVars) =>
      JSON.stringify({ legacy: true, apiKey: vars.apiKey, base: text }),
  );
  const upsertProvider = vi.fn(async (p: Provider) => p);
  return {
    validateAgentConfig,
    materializeAgentConfig,
    applyFormVars,
    upsertProvider,
    ...overrides,
  } as ReturnType<typeof mockDeps>;
}

describe('Catalog configSchemaVersion three-state', () => {
  it('number → required (must use projector)', () => {
    const exp = resolveProjectorExpectation({
      catalogStatus: 'ready',
      entry: entry('claude', 1),
    });
    expect(exp).toEqual({ kind: 'required', version: 1 });
    expect(planSchemaLoad(exp)).toEqual({ action: 'load_schema' });
  });

  it('null → unsupported (legacy applyFormVars allowed)', () => {
    const exp = resolveProjectorExpectation({
      catalogStatus: 'ready',
      entry: entry('cursor', null),
    });
    expect(exp).toEqual({ kind: 'unsupported' });
    expect(planSchemaLoad(exp)).toEqual({ action: 'unsupported' });
  });

  it('undefined field → unknown (fail closed, not unsupported)', () => {
    const exp = resolveProjectorExpectation({
      catalogStatus: 'ready',
      entry: entry('mystery'),
    });
    expect(exp.kind).toBe('unknown');
    expect(exp).toMatchObject({ reason: 'version_undefined' });
    expect(planSchemaLoad(exp).action).toBe('error');
  });

  it('catalog loading/idle → unknown wait (not unsupported)', () => {
    for (const status of ['idle', 'loading'] as const) {
      const exp = resolveProjectorExpectation({
        catalogStatus: status,
        entry: entry('claude', 1),
      });
      expect(exp).toEqual({ kind: 'unknown', reason: 'catalog_not_ready' });
      expect(planSchemaLoad(exp)).toEqual({ action: 'wait' });
    }
  });

  it('catalog error/unavailable or missing entry → error plan', () => {
    expect(
      planSchemaLoad(
        resolveProjectorExpectation({
          catalogStatus: 'error',
          entry: entry('claude', 1),
        }),
      ).action,
    ).toBe('error');
    expect(
      planSchemaLoad(
        resolveProjectorExpectation({
          catalogStatus: 'unavailable',
          entry: null,
        }),
      ).action,
    ).toBe('error');
    expect(
      planSchemaLoad(
        resolveProjectorExpectation({
          catalogStatus: 'ready',
          entry: null,
        }),
      ).action,
    ).toBe('error');
  });
});

describe('save gate by schema status', () => {
  it('only ready/unsupported allow save', () => {
    const allowed: SchemaUiStatus[] = ['ready', 'unsupported'];
    const blocked: SchemaUiStatus[] = ['idle', 'loading', 'error'];
    for (const s of allowed) {
      expect(canSaveWithSchemaStatus(s)).toBe(true);
      expect(resolveSavePath(s)).not.toBe('blocked');
    }
    for (const s of blocked) {
      expect(canSaveWithSchemaStatus(s)).toBe(false);
      expect(resolveSavePath(s)).toBe('blocked');
    }
  });

  it('canSave does not require model; fetch failure does not flip it', () => {
    const gate = {
      schemaStatus: 'ready' as const,
      configError: null,
      isEdit: false,
      apiKey: 'sk-test-key',
      piNeedsUrl: false,
      baseUrl: 'https://mytokens.cc',
      model: '',
    };
    expect(canSaveProviderForm(gate)).toBe(true);
    expect(canSaveProviderForm({ ...gate, model: undefined })).toBe(true);
    // fetch status is not an input — a failed listRemoteOpenAiModels cannot flip the gate
    expect(canSaveProviderForm({ ...gate, model: '   ' })).toBe(true);
    expect(
      canSaveProviderForm({
        ...gate,
        isEdit: true,
        apiKey: '',
        model: '',
      }),
    ).toBe(true);
    expect(canSaveProviderForm({ ...gate, apiKey: '' })).toBe(false);
    expect(
      canSaveProviderForm({
        ...gate,
        piNeedsUrl: true,
        baseUrl: '',
        model: 'sonnet',
      }),
    ).toBe(false);
  });

  it('empty custom model still upserts without inventing a model id', async () => {
    const vars = withDefaultModel(
      'claude',
      {
        ...EMPTY_FORM_VARS,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-test-key',
        model: '',
      },
      false,
    );
    expect(vars.model).toBe('');
    const deps = mockDeps();
    const result = await runProviderSaveFlow(
      baseInput({ vars, saveVars: vars, schemaStatus: 'ready' }),
      deps,
    );
    expect(result.ok).toBe(true);
    expect(deps.upsertProvider).toHaveBeenCalledOnce();
    expect(deps.materializeAgentConfig.mock.calls[0][1].model).toBe('');
  });

  it('loading/error path does not call materialize, applyFormVars, or upsert', async () => {
    for (const status of ['idle', 'loading', 'error'] as const) {
      const deps = mockDeps();
      const result = await runProviderSaveFlow(baseInput({ schemaStatus: status }), deps);
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.code).toBe('schema_not_ready');
        expect(result.preserveInput).toBe(true);
      }
      expect(deps.validateAgentConfig).not.toHaveBeenCalled();
      expect(deps.materializeAgentConfig).not.toHaveBeenCalled();
      expect(deps.applyFormVars).not.toHaveBeenCalled();
      expect(deps.upsertProvider).not.toHaveBeenCalled();
    }
  });
});

describe('projector path fail-closed', () => {
  it('projects shared form vars to the active backend schema', () => {
    const values = projectValuesToSchema(
      {
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
      {
        ...EMPTY_FORM_VARS,
        model: 'grok-4.5',
        baseUrl: 'https://relay.example.com/v1',
        apiKey: REDACTED_MARKER,
      },
    );
    expect(values).toEqual({
      model: 'grok-4.5',
      baseUrl: 'https://relay.example.com/v1',
      apiKey: REDACTED_MARKER,
    });
  });

  it('schema load failure is planned as error (no legacy fallback)', () => {
    // When Catalog requires projector, plan is load_schema — dialog maps load failure → error.
    // Legacy is only planned when expectation is unsupported.
    const required = resolveProjectorExpectation({
      catalogStatus: 'ready',
      entry: entry('claude', 1),
    });
    expect(planSchemaLoad(required)).toEqual({ action: 'load_schema' });
    expect(resolveSavePath('error')).toBe('blocked');
    expect(resolveSavePath('unsupported')).toBe('legacy');
  });

  it('invalid JSON does not save as {} and never upserts', async () => {
    const deps = mockDeps();
    const badJson = '{not-json';
    const parse = parseJsonConfigBase(badJson);
    expect(parse.ok).toBe(false);

    const result = await runProviderSaveFlow(
      baseInput({
        schemaStatus: 'ready',
        finalFormat: 'json',
        baseText: badJson,
        configText: badJson,
      }),
      deps,
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('invalid_json');
      expect(result.message).toMatch(/JSON/);
      expect(result.preserveInput).toBe(true);
    }
    expect(deps.validateAgentConfig).not.toHaveBeenCalled();
    expect(deps.materializeAgentConfig).not.toHaveBeenCalled();
    expect(deps.applyFormVars).not.toHaveBeenCalled();
    expect(deps.upsertProvider).not.toHaveBeenCalled();
  });

  it('structured edits preserve invalid intermediate text and expose a clear error', () => {
    const malformed = '{"env":{"CUSTOM":"keep-me"}';
    expect(getConfigTextError('claude', malformed, 'json')).toMatch(/JSON/);
    expect(
      applyFormVars('claude', malformed, 'json', {
        ...EMPTY_FORM_VARS,
        baseUrl: 'https://new.example.com',
        model: 'new-model',
      }),
    ).toBe(malformed);
    expect(getConfigTextError('claude', '{"unknown":true}', 'json')).toBeNull();
    expect(
      getConfigTextError(
        'claude',
        '{"baseURL":"https://openrouter.ai/api/v1","baseUrl":"https://openrouter.ai/api/v1"}',
        'json',
      ),
    ).toMatch(/ANTHROPIC_BASE_URL/);
  });

  it('JSON parse banner stays Chinese without SyntaxError English', () => {
    const tZh = createTranslator('zh');
    const malformed = '{"env":{"CUSTOM":"keep-me"}';
    const msg = getConfigTextError('claude', malformed, 'json', tZh);
    expect(msg).toMatch(/配置没法解析/);
    expect(msg).not.toMatch(/Unexpected|SyntaxError|JSON\.parse/i);
    expect(getConfigTextError('claude', malformed, 'json')).not.toMatch(
      /Unexpected|SyntaxError/i,
    );
  });

  it('validate ok=false does not materialize or upsert', async () => {
    const deps = mockDeps({
      validateAgentConfig: vi.fn(async () => ({
        ok: false,
        issues: [
          { fieldKey: 'apiKey', code: 'required', message: 'apiKey required' },
        ],
      })),
    });
    const result = await runProviderSaveFlow(baseInput({ schemaStatus: 'ready' }), deps);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('validation_failed');
      expect(result.issues).toHaveLength(1);
      expect(result.preserveInput).toBe(true);
    }
    expect(deps.validateAgentConfig).toHaveBeenCalledOnce();
    expect(deps.materializeAgentConfig).not.toHaveBeenCalled();
    expect(deps.applyFormVars).not.toHaveBeenCalled();
    expect(deps.upsertProvider).not.toHaveBeenCalled();
  });

  it('materialize failure does not applyFormVars or upsert', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async () => {
        throw new Error('projector crashed');
      }),
    });
    const result = await runProviderSaveFlow(baseInput({ schemaStatus: 'ready' }), deps);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('materialize_failed');
      expect(result.message).toMatch(/projector crashed/);
      expect(result.preserveInput).toBe(true);
    }
    expect(deps.validateAgentConfig).toHaveBeenCalledOnce();
    expect(deps.materializeAgentConfig).toHaveBeenCalledOnce();
    expect(deps.applyFormVars).not.toHaveBeenCalled();
    expect(deps.upsertProvider).not.toHaveBeenCalled();
  });

  it('unrecognized materialize result fails closed', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async () => null),
    });
    const result = await runProviderSaveFlow(baseInput({ schemaStatus: 'ready' }), deps);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('materialize_failed');
    }
    expect(deps.upsertProvider).not.toHaveBeenCalled();
    expect(deps.applyFormVars).not.toHaveBeenCalled();
  });

  it('successful projector path: validate → materialize → upsert (no applyFormVars)', async () => {
    const deps = mockDeps();
    const result = await runProviderSaveFlow(baseInput({ schemaStatus: 'ready' }), deps);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.path).toBe('projector');
      expect(result.provider.configText).toContain('sk-test-key');
    }
    expect(deps.validateAgentConfig).toHaveBeenCalledOnce();
    expect(deps.materializeAgentConfig).toHaveBeenCalledOnce();
    expect(deps.applyFormVars).not.toHaveBeenCalled();
    expect(deps.upsertProvider).toHaveBeenCalledOnce();
  });
});

describe('legacy path only when configSchemaVersion is null', () => {
  it('unsupported status calls applyFormVars then upsert; never validate/materialize', async () => {
    const deps = mockDeps();
    const result = await runProviderSaveFlow(
      baseInput({
        agentId: 'cursor',
        schemaStatus: 'unsupported',
      }),
      deps,
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.path).toBe('legacy');
      expect(result.provider.configText).toContain('legacy');
    }
    expect(deps.applyFormVars).toHaveBeenCalledOnce();
    expect(deps.upsertProvider).toHaveBeenCalledOnce();
    expect(deps.validateAgentConfig).not.toHaveBeenCalled();
    expect(deps.materializeAgentConfig).not.toHaveBeenCalled();
  });

  it('ready never falls back to applyFormVars even if materialize fails', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async () => {
        throw new Error('down');
      }),
    });
    await runProviderSaveFlow(baseInput({ schemaStatus: 'ready' }), deps);
    expect(deps.applyFormVars).not.toHaveBeenCalled();
  });
});

describe('secret unchanged semantics', () => {
  it('edit with empty apiKey keeps empty auth for codex (backend retains secret)', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async (_id, values, baseRaw) => ({
        format: 'toml',
        content: 'model = "gpt"\n',
        // no auth when secret not provided
        ...(typeof values.apiKey === 'string' &&
        values.apiKey &&
        values.apiKey !== REDACTED_MARKER
          ? { auth: { OPENAI_API_KEY: values.apiKey } }
          : {}),
        baseRaw,
      })),
    });
    const existing: Provider = {
      id: 'p1',
      agentId: 'codex',
      name: 'Codex',
      preset: 'custom',
      configText: 'model = "gpt"\n',
      configFormat: 'toml',
      authApiKey: REDACTED_MARKER,
      isCurrent: false,
    };
    const vars: ProviderFormVars = {
      ...EMPTY_FORM_VARS,
      model: 'gpt',
      apiKey: '', // leave blank → retain
    };
    const result = await runProviderSaveFlow(
      baseInput({
        agentId: 'codex',
        schemaStatus: 'ready',
        isEdit: true,
        existing,
        vars,
        saveVars: vars,
        finalFormat: 'toml',
        baseText: 'model = "gpt"\n',
        configFormat: 'toml',
      }),
      deps,
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      // empty string signals "unchanged" to pool upsert for codex edit
      expect(result.provider.authApiKey).toBe('');
    }
    const matArgs = deps.materializeAgentConfig.mock.calls[0];
    expect(matArgs[1].apiKey).toBe('');
  });

  it('materialize echoing *** does not overwrite the empty keep-secret signal', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async () => ({
        format: 'toml',
        content: 'model = "gpt"\n',
        auth: { OPENAI_API_KEY: REDACTED_MARKER },
      })),
    });
    const existing: Provider = {
      id: 'p1',
      agentId: 'codex',
      name: 'Codex',
      preset: 'custom',
      configText: 'model = "gpt"\n',
      configFormat: 'toml',
      authApiKey: REDACTED_MARKER,
      isCurrent: false,
    };
    const vars: ProviderFormVars = {
      ...EMPTY_FORM_VARS,
      model: 'gpt',
      apiKey: '',
    };
    const result = await runProviderSaveFlow(
      baseInput({
        agentId: 'codex',
        schemaStatus: 'ready',
        isEdit: true,
        existing,
        vars,
        saveVars: vars,
        finalFormat: 'toml',
        baseText: 'model = "gpt"\n',
        configFormat: 'toml',
      }),
      deps,
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.provider.authApiKey).toBe('');
    }
  });

  it('REDACTED_MARKER is not treated as a new secret in materialize input path', async () => {
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async (_id, values) => {
        if (values.apiKey === SECRET_REDACTED || values.apiKey === REDACTED_MARKER) {
          return { env: {}, kept: true };
        }
        return { env: { KEY: values.apiKey } };
      }),
    });
    const vars: ProviderFormVars = {
      ...EMPTY_FORM_VARS,
      apiKey: REDACTED_MARKER,
      model: 'm',
    };
    const result = await runProviderSaveFlow(
      baseInput({ vars, saveVars: vars, schemaStatus: 'ready' }),
      deps,
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.provider.configText).toContain('kept');
      expect(result.provider.configText).not.toContain('sk-');
    }
  });
});

describe('retry plan preserves readiness progression', () => {
  it('after schema error, successful re-plan from required still loads schema (form state external)', () => {
    // Dialog keeps form fields; only schemaLoadToken changes. Pure plan is stable:
    const exp = resolveProjectorExpectation({
      catalogStatus: 'ready',
      entry: entry('claude', 1),
    });
    expect(planSchemaLoad(exp).action).toBe('load_schema');
    // After load succeeds → ready allows save
    expect(canSaveWithSchemaStatus('ready')).toBe(true);
    expect(canSaveWithSchemaStatus('error')).toBe(false);
  });

  it('runProviderSaveFlow after simulated retry (ready) uses projector path with same user text', async () => {
    const userText = '{"env":{"CUSTOM":"keep-me"},"extra":1}';
    const deps = mockDeps({
      materializeAgentConfig: vi.fn(async (_id, values, baseRaw) => ({
        ...(baseRaw as object),
        model: values.model,
      })),
    });
    // First attempt blocked by error status (simulates pre-retry)
    const blocked = await runProviderSaveFlow(
      baseInput({ schemaStatus: 'error', baseText: userText, configText: userText }),
      deps,
    );
    expect(blocked.ok).toBe(false);
    expect(deps.upsertProvider).not.toHaveBeenCalled();

    // Retry succeeded → ready; same baseText preserved by caller
    const ok = await runProviderSaveFlow(
      baseInput({ schemaStatus: 'ready', baseText: userText, configText: userText }),
      deps,
    );
    expect(ok.ok).toBe(true);
    if (ok.ok) {
      expect(ok.provider.configText).toContain('keep-me');
      expect(ok.provider.configText).toContain('"extra": 1');
    }
  });
});

describe('mock Catalog aligns with mock ConfigPort projector support', () => {
  let configPort: ReturnType<typeof createMockConfigPort>;

  beforeEach(() => {
    configPort = createMockConfigPort();
  });

  it('claude/codex/kimi/grok have projector schemas and schema exists', async () => {
    const expectedVersions = { claude: 1, codex: 1, kimi: 1, grok: 2 } as const;
    for (const key of ['claude', 'codex', 'kimi', 'grok'] as const) {
      const row = MOCK_AGENT_CATALOG.find((e) => e.key === key);
      expect(row, key).toBeDefined();
      expect(row!.configSchemaVersion).toBe(expectedVersions[key]);
      const schema = await configPort.getAgentConfigSchema(key);
      expect(schema.schemaVersion).toBe(expectedVersions[key]);
      expect(schema.agentKey).toBe(key);
    }
  });

  it('places Codex model after the API key for the guided form', async () => {
    const schema = await configPort.getAgentConfigSchema('codex');
    expect(schema.fields.map((field) => field.key)).toEqual([
      'baseUrl',
      'apiKey',
      'model',
      'reasoningEffort',
      'wireApi',
      'providerSlug',
    ]);
  });

  it('cursor/pi/workbuddy have configSchemaVersion=null and no projector', async () => {
    for (const key of ['cursor', 'pi', 'workbuddy'] as const) {
      const row = MOCK_AGENT_CATALOG.find((e) => e.key === key);
      expect(row, key).toBeDefined();
      expect(row!.configSchemaVersion).toBeNull();
      await expect(configPort.getAgentConfigSchema(key)).rejects.toThrow(/unsupported/i);
    }
  });

  it('every catalog row with number version has mock schema; null has none', async () => {
    for (const row of MOCK_AGENT_CATALOG) {
      if (row.configSchemaVersion === null) {
        await expect(configPort.getAgentConfigSchema(row.key)).rejects.toThrow();
      } else if (typeof row.configSchemaVersion === 'number') {
        const schema = await configPort.getAgentConfigSchema(row.key);
        expect(schema.schemaVersion).toBe(row.configSchemaVersion);
      } else {
        // undefined must not appear in default mock catalog
        expect.fail(`catalog entry ${row.key} has undefined configSchemaVersion`);
      }
    }
  });
});
