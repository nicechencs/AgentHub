/**
 * # provider-detect — 供应商配置「智能识别 + 表单写回」（生产模块）
 *
 * ## 原则
 * - 只做**模式识别**：任意 URL / Key / 字段名，不绑定具体中转域名或密钥值
 * - **不包含**测试样例；样例仅在 `__tests__/fixtures/`，只给 vitest 用
 *
 * ## 用户配置流程（UI）
 * 1. 粘贴任意配置 → `applySmartPaste(agentId, paste)`
 * 2. 展示 `vars`（URL / Key / Model）与 `detect.hints`
 * 3. 用户微调后保存；切换时由 adapter 写 live
 *
 * ## 扩展识别样式
 * 1. 把**脱敏形态**样例放到 `__tests__/fixtures/`
 * 2. 在 `detectors.ts` / `claudeEnv.ts` / `codexToml.ts` 加规则（仍不写死 URL/Key）
 * 3. 在 `__tests__/` 补回归
 */

export type {
  FormFieldKey,
  ProviderConfigDetector,
  ProviderFormVars,
  SmartDetectResult,
  SmartPasteApplyResult,
} from './types';
export { EMPTY_FORM_VARS, REDACTED_MARKER, CLAUDE_MODEL_ROLE_ENV } from './types';
export type { ClaudeModelRole } from './types';

export { DETECTORS, registerDetector } from './detectors';
export { smartDetectUrlAndKey } from './detect';
export { defaultConfigScaffold, isLiveFilePath, liveConfigPaths } from './scaffold';

export {
  applyFormVars,
  extractFormVars,
  formFieldVisibility,
  FORM_FIELD_LABELS,
  looksRedactedOrPlaceholder,
  maskConfigSecrets,
  maskPasteSecrets,
  parseJsonObjectConfig,
  resolveFormApiKeyFromEditor,
  writableSecret,
} from './fields';
export type { JsonObjectParseResult } from './fields';
export {
  CLAUDE_FOREIGN_ROOT_KEYS,
  nativeConfigIssueMessage,
  stripClaudeForeignRootKeys,
  validateNativeConfigText,
} from './native-config';
export type { NativeConfigIssue } from './native-config';

export {
  applySmartPaste,
  initFormFromConfig,
  isStructuredConfigPaste,
  mergeDetectIntoVars,
} from './apply';

export {
  FALLBACK_CUSTOM_MODEL,
  defaultModelForAgent,
  filterRemoteModelsForAgent,
  looksLikeGrokModel,
  isLivePastedApiKey,
  looksLikeLast4Mask,
  maskApiKeyLast4,
  openaiModelsUrl,
  parseOpenAiModelList,
  remoteModelsStatusView,
  resolveModelForSave,
  resolveUpstreamBaseUrl,
  shouldFetchRemoteModels,
  withDefaultModel,
} from './remote-models';
export type {
  RemoteModelsStatusKind,
  RemoteModelsStatusLabelKey,
  RemoteModelsStatusView,
} from './remote-models';

export {
  extractAllClaudeEnv,
  extractClaudeSettingsDocument,
  pickClaudeDetectFields,
} from './claudeEnv';
export {
  extractCodexDetectFields,
  extractOpenAiApiKey,
  isCodexAuthJsonPaste,
  isCodexTomlPaste,
  stripCodexPasteNoise,
} from './codexToml';
export { extractGrokDetectFields, isGrokTomlPaste } from './grokToml';
