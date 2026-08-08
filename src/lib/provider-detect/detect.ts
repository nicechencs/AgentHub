import { DETECTORS } from './detectors';
import type { SmartDetectResult } from './types';
// SmartDetectResult used for claudeAuthEnv typing in locals

/**
 * 智能识别粘贴文本中的 Endpoint URL / API Key（及可选 model）。
 *
 * 流水线：按 DETECTORS 顺序尝试；专用样式先命中，最后 generic-mixed 兜底。
 * 合并策略：先到先得（已有字段不被后序 detector 覆盖）。
 */
export function smartDetectUrlAndKey(raw: string): SmartDetectResult {
  const text = raw.trim();
  const hints: string[] = [];
  const matchedDetectors: string[] = [];

  if (!text) return { hints };

  let baseUrl: string | undefined;
  let apiKey: string | undefined;
  let model: string | undefined;
  let reasoningEffort: string | undefined;
  let wireApi: string | undefined;
  let providerSlug: string | undefined;
  let envKey: string | undefined;
  let rawConfigText: string | undefined;
  let suggestedName: string | undefined;
  let claudeAuthEnv: SmartDetectResult['claudeAuthEnv'];
  let extraEnv: Record<string, string> | undefined;

  for (const det of DETECTORS) {
    let hit: ReturnType<typeof det.extract>;
    try {
      hit = det.extract(text);
    } catch {
      continue;
    }
    if (!hit) continue;

    const contributed =
      (hit.baseUrl && !baseUrl) ||
      (hit.apiKey && !apiKey) ||
      (hit.model && !model) ||
      (hit.rawConfigText && !rawConfigText) ||
      (hit.extraEnv && Object.keys(hit.extraEnv).length > 0);
    if (
      !contributed &&
      !hit.suggestedName &&
      !hit.claudeAuthEnv &&
      !hit.reasoningEffort &&
      !hit.wireApi &&
      !hit.envKey
    ) {
      continue;
    }

    matchedDetectors.push(det.id);
    if (!baseUrl && hit.baseUrl) baseUrl = hit.baseUrl;
    if (!apiKey && hit.apiKey) apiKey = hit.apiKey;
    if (!model && hit.model) model = hit.model;
    if (!reasoningEffort && hit.reasoningEffort) reasoningEffort = hit.reasoningEffort;
    if (!wireApi && hit.wireApi) wireApi = hit.wireApi;
    if (!providerSlug && hit.providerSlug) providerSlug = hit.providerSlug;
    if (!envKey && hit.envKey) envKey = hit.envKey;
    if (!rawConfigText && hit.rawConfigText) rawConfigText = hit.rawConfigText;
    if (!suggestedName && hit.suggestedName) suggestedName = hit.suggestedName;
    if (!claudeAuthEnv && hit.claudeAuthEnv) claudeAuthEnv = hit.claudeAuthEnv;
    if (hit.extraEnv) {
      extraEnv = { ...(extraEnv ?? {}), ...hit.extraEnv };
    }

    // plain-url / plain-api-key 已足够则提前结束
    if (det.id === 'plain-url' || det.id === 'plain-api-key') break;
    // 完整 codex toml 已拿到正文
    if (det.id === 'codex-config-toml' && rawConfigText) break;
    // shell export / settings 已同时拿到 url+key 可结束
    if (baseUrl && apiKey && det.id.startsWith('claude-')) break;
    if (baseUrl && apiKey && det.id !== 'generic-mixed' && !det.id.startsWith('codex-'))
      break;
  }

  if (matchedDetectors.includes('plain-url') && baseUrl) {
    hints.push('整段识别为 Endpoint URL');
  } else if (matchedDetectors.includes('plain-api-key') && apiKey) {
    hints.push('整段识别为 API Key');
  } else {
    if (baseUrl) hints.push(`识别到 URL：${baseUrl}`);
    if (apiKey) {
      const tail = apiKey.length > 8 ? apiKey.slice(-4) : '';
      hints.push(`识别到 API Key${tail ? `（…${tail}）` : ''}`);
    }
    if (model) hints.push(`识别到 Model：${model}`);
    if (providerSlug) hints.push(`Provider：${providerSlug}`);
    if (envKey) hints.push(`env_key：${envKey}`);
    if (reasoningEffort) hints.push(`reasoning：${reasoningEffort}`);
    if (extraEnv && Object.keys(extraEnv).length) {
      hints.push(`附加 env：${Object.keys(extraEnv).join(', ')}`);
    }
    if (rawConfigText) hints.push('已保留完整 config.toml');
  }

  if (!baseUrl && !apiKey && !rawConfigText) {
    hints.push('未识别到 URL 或 API Key，请手动填写');
  }

  return {
    baseUrl,
    apiKey,
    model,
    reasoningEffort,
    wireApi,
    providerSlug,
    envKey,
    rawConfigText,
    extraEnv,
    claudeAuthEnv,
    suggestedName: suggestedName ?? (baseUrl ? hostFallback(baseUrl) : undefined),
    hints,
    matchedDetectors,
  };
}

function hostFallback(url: string): string | undefined {
  try {
    return new URL(url).host || undefined;
  } catch {
    return undefined;
  }
}
