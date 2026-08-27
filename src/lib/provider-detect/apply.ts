/**
 * 一站式：粘贴识别 + 合并表单 + 写回配置正文。
 * UI 只调此函数即可完成「智能识别并填入」。
 */
import { parseContextWindowChoice } from '@/lib/claude-client-env';
import type { AgentId } from '@/lib/types';
import { smartDetectUrlAndKey } from './detect';
import { applyFormVars, extractFormVars } from './fields';
import { isGrokTomlPaste } from './grokToml';
import { defaultConfigScaffold } from './scaffold';
import {
  CLAUDE_MODEL_ROLE_ENV,
  EMPTY_FORM_VARS,
  REDACTED_MARKER,
  type ProviderFormVars,
  type SmartPasteApplyResult,
} from './types';

function ensureBaseText(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
): { text: string; format: 'json' | 'toml' } {
  if (configText.trim() && configText.trim() !== REDACTED_MARKER) {
    return { text: configText, format };
  }
  const scaffold = defaultConfigScaffold(agentId);
  return { text: scaffold.text, format: scaffold.format };
}

const STRUCTURED_PASTE_DETECTORS = new Set([
  'claude-settings-json',
  'claude-shell-export',
  'claude-cmd-set',
  'claude-powershell-env',
  'claude-env-generic',
  'codex-config-toml',
  'grok-config-toml',
]);

function isKimiTomlPaste(text: string): boolean {
  return /^\s*\[providers\./im.test(text) && /base_url\s*=/i.test(text);
}

function isJsonObjectPaste(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed.startsWith('{')) return false;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return Boolean(parsed && typeof parsed === 'object' && !Array.isArray(parsed));
  } catch {
    return false;
  }
}

/**
 * Complete config blobs (settings.json / env export / provider TOML) replace
 * omitted model fields. URL/key fragments keep whatever is already in the form.
 */
export function isStructuredConfigPaste(
  detect: ReturnType<typeof smartDetectUrlAndKey>,
  paste: string,
  agentId: AgentId,
): boolean {
  if (detect.rawConfigText?.trim()) return true;
  if (detect.matchedDetectors?.some((id) => STRUCTURED_PASTE_DETECTORS.has(id))) {
    return true;
  }
  if (agentId === 'kimi' && isKimiTomlPaste(paste)) return true;
  if (
    (agentId === 'claude' || agentId === 'pi' || agentId === 'workbuddy') &&
    isJsonObjectPaste(paste)
  ) {
    return true;
  }
  return false;
}

/**
 * 合并智能识别结果到已有表单变量。
 * - 默认：识别到的字段**覆盖**（粘贴意图是填入/更新）
 * - `preferExisting: true`：仅填空槽（高级合并场景）
 * - `replaceOmittedModel: true`：粘贴里没写模型名时清空表单里的旧模型，不沿用官方默认
 */
export function mergeDetectIntoVars(
  vars: ProviderFormVars,
  detect: ReturnType<typeof smartDetectUrlAndKey>,
  opts?: { preferExisting?: boolean; replaceOmittedModel?: boolean },
): ProviderFormVars {
  const keep = opts?.preferExisting === true;
  const replaceModel = opts?.replaceOmittedModel === true && !keep;
  const extra = detect.extraEnv ?? {};
  const pastedModel = detect.model ?? extra.ANTHROPIC_MODEL;
  const pickRole = (envKey: string, current: string) => {
    if (keep && current.trim()) return current;
    if (replaceModel) return extra[envKey] ?? '';
    return extra[envKey] ?? current;
  };
  return {
    ...vars,
    baseUrl:
      keep && vars.baseUrl.trim()
        ? vars.baseUrl
        : detect.baseUrl ?? vars.baseUrl,
    apiKey:
      keep && vars.apiKey.trim() ? vars.apiKey : detect.apiKey ?? vars.apiKey,
    model:
      keep && vars.model.trim()
        ? vars.model
        : pastedModel ?? (replaceModel ? '' : vars.model),
    modelOpus: pickRole(CLAUDE_MODEL_ROLE_ENV.opus, vars.modelOpus),
    modelSonnet: pickRole(CLAUDE_MODEL_ROLE_ENV.sonnet, vars.modelSonnet),
    modelHaiku: pickRole(CLAUDE_MODEL_ROLE_ENV.haiku, vars.modelHaiku),
    modelFable: pickRole(CLAUDE_MODEL_ROLE_ENV.fable, vars.modelFable),
    modelSubagent: pickRole(CLAUDE_MODEL_ROLE_ENV.subagent, vars.modelSubagent),
    contextWindow:
      keep && vars.contextWindow.trim()
        ? vars.contextWindow
        : extra.CLAUDE_CODE_MAX_CONTEXT_TOKENS != null
          ? parseContextWindowChoice(extra.CLAUDE_CODE_MAX_CONTEXT_TOKENS)
          : replaceModel
            ? ''
            : vars.contextWindow,
    claudeAuthEnv: detect.claudeAuthEnv ?? vars.claudeAuthEnv,
  };
}

/**
 * 粘贴文本 → 识别 URL/Key → 写回 agent 配置正文。
 *
 * @param agentId 当前 Agent
 * @param paste 用户粘贴的任意配置文本
 * @param current 可选：当前编辑中的 config / vars（编辑模式）
 */
export function applySmartPaste(
  agentId: AgentId,
  paste: string,
  current?: {
    configText?: string;
    configFormat?: 'json' | 'toml';
    vars?: ProviderFormVars;
    /** true：已有非空字段不被识别结果覆盖 */
    preferExisting?: boolean;
  },
): SmartPasteApplyResult {
  const detect = smartDetectUrlAndKey(paste);
  const scaffold = defaultConfigScaffold(agentId);
  const initialFormat = current?.configFormat ?? scaffold.format;
  const base = ensureBaseText(agentId, current?.configText ?? '', initialFormat);
  const prevVars =
    current?.vars ??
    extractFormVars(agentId, base.text, base.format) ??
    { ...EMPTY_FORM_VARS };

  const replaceOmittedModel = isStructuredConfigPaste(detect, paste, agentId);
  const vars = mergeDetectIntoVars(prevVars, detect, {
    preferExisting: current?.preferExisting,
    replaceOmittedModel,
  });
  if (detect.claudeAuthEnv) {
    vars.claudeAuthEnv = detect.claudeAuthEnv;
  }
  if (detect.reasoningEffort) vars.reasoningEffort = detect.reasoningEffort;
  if (detect.wireApi) vars.wireApi = detect.wireApi;
  if (detect.providerSlug) vars.providerSlug = detect.providerSlug;

  // Complete native documents replace the current body so leftover official
  // models / `$schema` / provider tables from the form are not mixed in.
  let configBase = base.text;
  let outFormat: 'json' | 'toml' = base.format;
  if (agentId === 'claude' && detect.rawConfigText?.trim()) {
    configBase = detect.rawConfigText;
    outFormat = 'json';
  } else if (agentId === 'codex' && detect.rawConfigText?.trim()) {
    configBase = detect.rawConfigText;
    outFormat = 'toml';
  } else if (
    agentId === 'grok' &&
    (detect.rawConfigText?.trim() || isGrokTomlPaste(paste))
  ) {
    const grokBody = (detect.rawConfigText ?? paste).trim();
    configBase = grokBody.endsWith('\n') ? grokBody : `${grokBody}\n`;
    outFormat = 'toml';
  } else if (agentId === 'kimi' && isKimiTomlPaste(paste)) {
    const trimmed = paste.trim();
    configBase = trimmed.endsWith('\n') ? trimmed : `${trimmed}\n`;
    outFormat = 'toml';
  }

  const configText = applyFormVars(agentId, configBase, outFormat, vars, {
    extraEnv: detect.extraEnv,
  });

  return {
    detect,
    vars,
    configText,
    configFormat: outFormat,
    suggestedName: detect.suggestedName,
  };
}

/** 从已有 Provider 配置初始化表单（含 authApiKey） */
export function initFormFromConfig(
  agentId: AgentId,
  configText: string,
  configFormat: 'json' | 'toml',
  authApiKey?: string,
): ProviderFormVars {
  const vars = extractFormVars(agentId, configText, configFormat);
  if (agentId === 'codex' && authApiKey) {
    vars.apiKey = authApiKey === REDACTED_MARKER ? '' : authApiKey;
  }
  return vars;
}
