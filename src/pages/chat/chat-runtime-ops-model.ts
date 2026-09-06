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
): string[] {
  const id = modelId?.trim();
  if (!id) return [];
  return models.find((item) => item.id === id)?.efforts ?? [];
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
): RuntimeTurnSettings {
  if (models.length === 0) {
    return {
      model: settings.model?.trim() || null,
      effort: settings.effort?.trim() || null,
    };
  }
  const model = settings.model?.trim() || null;
  if (!model) {
    return { model: null, effort: null };
  }
  const option = models.find((item) => item.id === model);
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
): RuntimeTurnSettings {
  const model = modelId.trim();
  const option = models.find((item) => item.id === model);
  return {
    model,
    effort: defaultEffortForModel(option),
  };
}
