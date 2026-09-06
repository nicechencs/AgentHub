import type {
  RuntimeExtensionItem,
  RuntimeModelOption,
  RuntimeOptions,
  RuntimeTurnSettings,
} from '@/lib/api/chat';

export type RuntimeCatalogMemory = {
  conversationId: string | null;
  models: RuntimeModelOption[];
  extensions: RuntimeExtensionItem[];
};


/** Session memory for efforts Codex rejected despite advertising them in model/list. */
export type DeniedEffortMemory = Record<string, string[]>;

const sessionDeniedEfforts: DeniedEffortMemory = {};

export function resetDeniedEffortsForTests(): void {
  for (const key of Object.keys(sessionDeniedEfforts)) {
    delete sessionDeniedEfforts[key];
  }
}

export function listDeniedEfforts(): DeniedEffortMemory {
  const out: DeniedEffortMemory = {};
  for (const [model, efforts] of Object.entries(sessionDeniedEfforts)) {
    out[model] = [...efforts];
  }
  return out;
}

export function noteDeniedEffort(modelId: string, effort: string): boolean {
  const model = modelId.trim();
  const value = effort.trim();
  if (!model || !value) return false;
  const current = sessionDeniedEfforts[model] ?? [];
  if (current.includes(value)) return false;
  sessionDeniedEfforts[model] = [...current, value];
  return true;
}

export function applyDeniedEfforts(
  models: RuntimeModelOption[],
  denied: DeniedEffortMemory = sessionDeniedEfforts,
): RuntimeModelOption[] {
  return models.map((option) => {
    const blocked = new Set(denied[option.id] ?? []);
    if (blocked.size === 0) return option;
    const efforts = option.efforts.filter((item) => !blocked.has(item));
    const fallback = option.defaultEffort?.trim();
    const defaultEffort =
      fallback && efforts.includes(fallback) ? fallback : efforts[0] ?? null;
    return { ...option, efforts, defaultEffort };
  });
}

/** True when failure text matches chat.failure.thinkingUnsupported mapping. */
export function isThinkingUnsupportedFailure(text: string | null | undefined): boolean {
  const hay = (text ?? '').toLowerCase();
  return (
    hay.includes('reasoningeffort')
    || hay.includes('reasoning_effort')
    || hay.includes('does not support parameter')
    || hay.includes('不支持思考强度')
    || hay.includes('不支持当前思考设置')
  );
}

/**
 * Learn from a thinkingUnsupported failure for the current settings pair.
 * Returns coerced settings when the current effort was denied.
 */
export function learnFromThinkingUnsupported(
  settings: RuntimeTurnSettings,
  models: RuntimeModelOption[],
  errorText: string | null | undefined,
): { models: RuntimeModelOption[]; settings: RuntimeTurnSettings; learned: boolean } {
  if (!isThinkingUnsupportedFailure(errorText)) {
    return { models, settings, learned: false };
  }
  const model = settings.model?.trim() || '';
  const effort = settings.effort?.trim() || '';
  if (!model || !effort) {
    return { models, settings, learned: false };
  }
  const learned = noteDeniedEffort(model, effort);
  const nextModels = applyDeniedEfforts(models);
  const nextSettings = coerceSettingsToCatalog(settings, nextModels);
  return { models: nextModels, settings: nextSettings, learned };
}


/**
 * Keep the last non-empty catalog for the same conversation when a frozen
 * options read returns empty lists (never fetched / mid-turn no-spawn).
 */
export function retainRuntimeCatalog(
  prior: RuntimeCatalogMemory,
  next: RuntimeOptions,
  conversationId: string,
): Pick<RuntimeCatalogMemory, 'models' | 'extensions'> {
  const sameConversation = prior.conversationId === conversationId;
  const nextEmpty = next.models.length === 0 && next.extensions.length === 0;
  const priorUseful = prior.models.length > 0 || prior.extensions.length > 0;
  if (next.settingsFrozen && nextEmpty && sameConversation && priorUseful) {
    return { models: prior.models, extensions: prior.extensions };
  }
  return { models: next.models, extensions: next.extensions };
}

export function effortsForModel(
  models: RuntimeModelOption[],
  modelId: string | null | undefined,
  denied: DeniedEffortMemory = sessionDeniedEfforts,
): string[] {
  const id = modelId?.trim();
  if (!id) return [];
  const effective = applyDeniedEfforts(models, denied);
  return effective.find((item) => item.id === id)?.efforts ?? [];
}

/** Prefer catalog default when it is supported; otherwise first supported effort. */
export function defaultEffortForModel(option: RuntimeModelOption | undefined): string | null {
  if (!option || option.efforts.length === 0) return null;
  const fallback = option.defaultEffort?.trim();
  if (fallback && option.efforts.includes(fallback)) return fallback;
  return option.efforts[0] ?? null;
}

export function isEffortCompatible(
  option: RuntimeModelOption | undefined,
  effort: string | null | undefined,
): boolean {
  const value = effort?.trim();
  if (!value) return true;
  if (!option) return false;
  if (option.efforts.length === 0) return false;
  return option.efforts.includes(value);
}

/**
 * Drive next-turn settings from model/list rows.
 * Unsupported effort is replaced by the model default / first supported value.
 * Does not invent models that are missing from the catalog.
 */
export function coerceSettingsToCatalog(
  settings: RuntimeTurnSettings,
  models: RuntimeModelOption[],
  denied: DeniedEffortMemory = sessionDeniedEfforts,
): RuntimeTurnSettings {
  const effective = applyDeniedEfforts(models, denied);
  if (effective.length === 0) {
    return {
      model: settings.model?.trim() || null,
      effort: settings.effort?.trim() || null,
    };
  }
  const model = settings.model?.trim() || null;
  if (!model) {
    return { model: null, effort: null };
  }
  const option = effective.find((item) => item.id === model);
  if (!option) {
    return { model, effort: settings.effort?.trim() || null };
  }
  if (isEffortCompatible(option, settings.effort)) {
    // Keep an omitted effort omitted — only model-switch / setSettings fill defaults.
    return { model, effort: settings.effort?.trim() || null };
  }
  return { model, effort: defaultEffortForModel(option) };
}

/** Settings payload for a model switch: never keep the previous model's effort. */
export function settingsForModelSwitch(
  modelId: string,
  models: RuntimeModelOption[],
  denied: DeniedEffortMemory = sessionDeniedEfforts,
): RuntimeTurnSettings {
  const model = modelId.trim();
  const option = applyDeniedEfforts(models, denied).find((item) => item.id === model);
  return {
    model,
    effort: defaultEffortForModel(option),
  };
}
