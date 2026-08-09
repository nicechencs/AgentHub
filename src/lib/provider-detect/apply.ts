/**
 * 一站式：粘贴识别 + 合并表单 + 写回配置正文。
 * UI 只调此函数即可完成「智能识别并填入」。
 */
import type { AgentId } from '@/lib/types';
import { smartDetectUrlAndKey } from './detect';
import { applyFormVars, extractFormVars } from './fields';
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

/**
 * 合并智能识别结果到已有表单变量。
 * - 默认：识别到的字段**覆盖**（粘贴意图是填入/更新）
 * - `preferExisting: true`：仅填空槽（高级合并场景）
 */
export function mergeDetectIntoVars(
  vars: ProviderFormVars,
  detect: ReturnType<typeof smartDetectUrlAndKey>,
  opts?: { preferExisting?: boolean },
): ProviderFormVars {
  const keep = opts?.preferExisting === true;
  const extra = detect.extraEnv ?? {};
  const pickRole = (envKey: string, current: string) => {
    if (keep && current.trim()) return current;
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
        : detect.model ?? extra.ANTHROPIC_MODEL ?? vars.model,
    modelOpus: pickRole(CLAUDE_MODEL_ROLE_ENV.opus, vars.modelOpus),
    modelSonnet: pickRole(CLAUDE_MODEL_ROLE_ENV.sonnet, vars.modelSonnet),
    modelHaiku: pickRole(CLAUDE_MODEL_ROLE_ENV.haiku, vars.modelHaiku),
    modelFable: pickRole(CLAUDE_MODEL_ROLE_ENV.fable, vars.modelFable),
    modelSubagent: pickRole(CLAUDE_MODEL_ROLE_ENV.subagent, vars.modelSubagent),
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

  const vars = mergeDetectIntoVars(prevVars, detect, {
    preferExisting: current?.preferExisting,
  });
  if (detect.claudeAuthEnv) {
    vars.claudeAuthEnv = detect.claudeAuthEnv;
  }
  if (detect.reasoningEffort) vars.reasoningEffort = detect.reasoningEffort;
  if (detect.wireApi) vars.wireApi = detect.wireApi;
  if (detect.providerSlug) vars.providerSlug = detect.providerSlug;

  // Codex/Grok：完整 TOML 粘贴优先保留全文，再叠表单字段。
  // This keeps provider-specific tables and newer native options intact.
  let configBase = base.text;
  let outFormat: 'json' | 'toml' = base.format;
  if ((agentId === 'codex' || agentId === 'grok') && detect.rawConfigText?.trim()) {
    configBase = detect.rawConfigText;
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
