/**
 * @deprecated 使用 `@/lib/ui-preferences`（UiPreferencesStore）。
 * 保留 re-export 以免页面渐进迁移时大面积改 import。
 */
export {
  StorageKey,
  loadJson,
  saveJson,
  loadString,
  saveString,
  loadBool,
  saveBool,
} from '@/lib/ui-preferences';
