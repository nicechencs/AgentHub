import { beforeEach, describe, expect, it } from 'vitest';
import type { RuntimeOptions } from '@/lib/api/chat';
import {
  applyDeniedEfforts,
  coerceSettingsToCatalog,
  defaultEffortForModel,
  effortsForModel,
  isEffortCompatible,
  learnFromThinkingUnsupported,
  resetDeniedEffortsForTests,
  retainRuntimeCatalog,
  settingsForModelSwitch,
} from './chat-runtime-ops-model';

const options = (
  partial: Partial<RuntimeOptions> & Pick<RuntimeOptions, 'settingsFrozen' | 'models' | 'extensions'>,
): RuntimeOptions => ({
  conversationId: 'c1',
  settings: {},
  modelsFromCodex: false,
  ...partial,
});

describe('retainRuntimeCatalog', () => {
  it('keeps prior lists when a frozen read returns empty for the same conversation', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'gpt-a', efforts: ['low'], defaultEffort: 'low' }],
      extensions: [
        {
          id: '/s/a',
          name: 'a',
          kind: 'skill' as const,
          installed: true,
          enabled: true,
          loaded: false,
          callable: true,
          path: '/s/a',
        },
      ],
    };
    const next = options({
      settingsFrozen: true,
      models: [],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1')).toEqual({
      models: prior.models,
      extensions: prior.extensions,
    });
  });

  it('does not keep prior lists across conversations', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'gpt-a', efforts: [], defaultEffort: null }],
      extensions: [],
    };
    const next = options({
      conversationId: 'c2',
      settingsFrozen: true,
      models: [],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c2')).toEqual({
      models: [],
      extensions: [],
    });
  });

  it('accepts a fresh non-empty catalog even while frozen', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'old', efforts: [], defaultEffort: null }],
      extensions: [],
    };
    const next = options({
      settingsFrozen: true,
      models: [{ id: 'new', efforts: ['high'], defaultEffort: 'high' }],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1').models[0]?.id).toBe('new');
  });

  it('replaces prior empty memory when idle returns a catalog', () => {
    const prior = { conversationId: 'c1', models: [], extensions: [] };
    const next = options({
      settingsFrozen: false,
      models: [{ id: 'gpt-b', efforts: [], defaultEffort: null }],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1').models[0]?.id).toBe('gpt-b');
  });
});

describe('model × effort compatibility', () => {
  const spark = {
    id: 'gpt-5.3-codex-spark',
    efforts: ['low', 'high'],
    defaultEffort: 'low',
  };
  const full = {
    id: 'gpt-full',
    efforts: ['low', 'medium', 'high'],
    defaultEffort: 'medium',
  };

  it('filters efforts to the selected model only', () => {
    expect(effortsForModel([spark, full], 'gpt-5.3-codex-spark')).toEqual(['low', 'high']);
    expect(effortsForModel([spark, full], 'missing')).toEqual([]);
    expect(effortsForModel([spark, full], null)).toEqual([]);
  });

  it('prefers supported default then first effort', () => {
    expect(defaultEffortForModel(spark)).toBe('low');
    expect(
      defaultEffortForModel({
        id: 'x',
        efforts: ['low', 'high'],
        defaultEffort: 'medium',
      }),
    ).toBe('low');
    expect(defaultEffortForModel({ id: 'none', efforts: [], defaultEffort: 'medium' })).toBeNull();
  });

  it('detects incompatible effort pairs', () => {
    expect(isEffortCompatible(spark, 'medium')).toBe(false);
    expect(isEffortCompatible(spark, 'low')).toBe(true);
    expect(isEffortCompatible(spark, null)).toBe(true);
    expect(isEffortCompatible({ id: 'n', efforts: [], defaultEffort: null }, 'low')).toBe(false);
  });

  it('resets unsupported effort when coercing to the catalog', () => {
    expect(
      coerceSettingsToCatalog(
        { model: 'gpt-5.3-codex-spark', effort: 'medium' },
        [spark, full],
      ),
    ).toEqual({ model: 'gpt-5.3-codex-spark', effort: 'low' });
  });

  it('keeps an omitted effort omitted when the pair is otherwise compatible', () => {
    expect(coerceSettingsToCatalog({ model: 'gpt-full' }, [spark, full])).toEqual({
      model: 'gpt-full',
      effort: null,
    });
  });

  it('resets effort on model switch instead of keeping the prior value', () => {
    expect(settingsForModelSwitch('gpt-5.3-codex-spark', [spark, full])).toEqual({
      model: 'gpt-5.3-codex-spark',
      effort: 'low',
    });
    expect(settingsForModelSwitch('gpt-full', [spark, full])).toEqual({
      model: 'gpt-full',
      effort: 'medium',
    });
  });
});


describe('over-reported catalog + learn-from-reject', () => {
  beforeEach(() => {
    resetDeniedEffortsForTests();
  });

  const overReportedSpark = {
    id: 'gpt-5.3-codex-spark',
    efforts: ['low', 'medium', 'high', 'xhigh'],
    defaultEffort: 'high',
  };

  it('keeps over-reported efforts until a thinkingUnsupported failure is learned', () => {
    expect(effortsForModel([overReportedSpark], 'gpt-5.3-codex-spark')).toEqual([
      'low',
      'medium',
      'high',
      'xhigh',
    ]);
    expect(settingsForModelSwitch('gpt-5.3-codex-spark', [overReportedSpark])).toEqual({
      model: 'gpt-5.3-codex-spark',
      effort: 'high',
    });
  });

  it('filters denied efforts after learning from the localized failure path', () => {
    const learned = learnFromThinkingUnsupported(
      { model: 'gpt-5.3-codex-spark', effort: 'medium' },
      [overReportedSpark],
      'OpenAI API error (400): does not support parameter reasoningEffort',
    );
    expect(learned.learned).toBe(true);
    expect(learned.models[0]?.efforts).toEqual(['low', 'high', 'xhigh']);
    expect(learned.settings).toEqual({ model: 'gpt-5.3-codex-spark', effort: 'high' });
    expect(effortsForModel(learned.models, 'gpt-5.3-codex-spark')).toEqual([
      'low',
      'high',
      'xhigh',
    ]);
    expect(
      coerceSettingsToCatalog(
        { model: 'gpt-5.3-codex-spark', effort: 'medium' },
        learned.models,
      ),
    ).toEqual({ model: 'gpt-5.3-codex-spark', effort: 'high' });
  });

  it('also learns from the Chinese localized copy', () => {
    const learned = learnFromThinkingUnsupported(
      { model: 'gpt-5.3-codex-spark', effort: 'medium' },
      [overReportedSpark],
      '这个模型不支持当前思考设置。请点重试。',
    );
    expect(learned.learned).toBe(true);
    expect(applyDeniedEfforts([overReportedSpark])[0]?.efforts).toEqual(['low', 'high', 'xhigh']);
  });
});
