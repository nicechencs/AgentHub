import type { DetectedApiEndpointType, RoutePoolSurface } from '@/lib/backend/contracts';
import type { AgentId } from '@/lib/types';

export type PoolAccessAgent = 'claude' | 'codex' | 'grok';

export type PoolApiChoiceType =
  | 'claudeMessages'
  | 'openaiResponses'
  | 'grokResponses'
  | 'openaiChatCompletions';

export type PoolApiChoice = {
  agentId: PoolAccessAgent;
  type: PoolApiChoiceType;
  endpoint: '/v1/messages' | '/v1/responses' | '/v1/chat/completions';
  grokApiBackend?: 'responses' | 'chat_completions';
  available: boolean;
};

export const API_CHOICES = [
  { agentId: 'claude', type: 'claudeMessages', endpoint: '/v1/messages' },
  { agentId: 'codex', type: 'openaiResponses', endpoint: '/v1/responses' },
  { agentId: 'grok', type: 'grokResponses', endpoint: '/v1/responses', grokApiBackend: 'responses' },
  {
    agentId: 'grok',
    type: 'openaiChatCompletions',
    endpoint: '/v1/chat/completions',
    grokApiBackend: 'chat_completions',
  },
] as const satisfies readonly {
  agentId: PoolAccessAgent;
  type: PoolApiChoiceType;
  endpoint: PoolApiChoice['endpoint'];
  grokApiBackend?: PoolApiChoice['grokApiBackend'];
}[];

export const DETECTED_API_CHOICES: Record<DetectedApiEndpointType, readonly PoolApiChoiceType[]> = {
  messages: ['claudeMessages'],
  responses: ['openaiResponses', 'grokResponses'],
  chat_completions: ['openaiChatCompletions'],
};

export type ApiVendorLabelKey =
  | 'routes.pool.page.apiVendorAnthropic'
  | 'routes.pool.page.apiVendorZhipuBigModel'
  | 'routes.pool.page.apiVendorKimiCn'
  | 'routes.pool.page.apiVendorKimiGlobal'
  | 'routes.pool.page.apiVendorOpenai'
  | 'routes.pool.page.apiVendorDeepseek'
  | 'routes.pool.page.apiVendorXai'
  | 'routes.pool.page.apiVendorQwenCn'
  | 'routes.pool.page.apiVendorQwenSingapore'
  | 'routes.pool.page.apiVendorQwenUs'
  | 'routes.pool.page.apiVendorZhipuZai'
  | 'routes.pool.page.apiVendorOpenRouter'
  | 'routes.pool.page.apiVendorNvidia'
  | 'routes.pool.page.apiVendorGroq'
  | 'routes.pool.page.apiVendorGemini'
  | 'routes.pool.page.apiVendorMistral';

export type ApiVendorPreset = {
  id: string;
  labelKey: ApiVendorLabelKey;
  endpoints: Partial<Record<PoolApiChoiceType, string>>;
};

const PRIMARY_ENDPOINT_ORDER = [
  'openaiChatCompletions',
  'openaiResponses',
  'grokResponses',
  'claudeMessages',
] as const satisfies readonly PoolApiChoiceType[];

/** One row per vendor, with the actual service URL for each supported API type. */
export const API_VENDORS: readonly ApiVendorPreset[] = [
  {
    id: 'anthropic',
    labelKey: 'routes.pool.page.apiVendorAnthropic',
    endpoints: { claudeMessages: 'https://api.anthropic.com' },
  },
  {
    id: 'openai',
    labelKey: 'routes.pool.page.apiVendorOpenai',
    endpoints: {
      openaiResponses: 'https://api.openai.com/v1',
      openaiChatCompletions: 'https://api.openai.com/v1',
    },
  },
  {
    id: 'xai',
    labelKey: 'routes.pool.page.apiVendorXai',
    endpoints: {
      grokResponses: 'https://api.x.ai/v1',
      openaiChatCompletions: 'https://api.x.ai/v1',
    },
  },
  {
    id: 'deepseek',
    labelKey: 'routes.pool.page.apiVendorDeepseek',
    endpoints: {
      claudeMessages: 'https://api.deepseek.com/anthropic',
      openaiResponses: 'https://api.deepseek.com',
      openaiChatCompletions: 'https://api.deepseek.com',
    },
  },
  {
    id: 'qwen-cn',
    labelKey: 'routes.pool.page.apiVendorQwenCn',
    endpoints: {
      claudeMessages: 'https://dashscope.aliyuncs.com/apps/anthropic',
      openaiChatCompletions: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    },
  },
  {
    id: 'qwen-sg',
    labelKey: 'routes.pool.page.apiVendorQwenSingapore',
    endpoints: {
      claudeMessages: 'https://dashscope-intl.aliyuncs.com/apps/anthropic',
      openaiChatCompletions: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',
    },
  },
  {
    id: 'qwen-us',
    labelKey: 'routes.pool.page.apiVendorQwenUs',
    endpoints: {
      claudeMessages: 'https://dashscope-us.aliyuncs.com/apps/anthropic',
      openaiChatCompletions: 'https://dashscope-us.aliyuncs.com/compatible-mode/v1',
    },
  },
  {
    id: 'zhipu-bigmodel',
    labelKey: 'routes.pool.page.apiVendorZhipuBigModel',
    endpoints: {
      claudeMessages: 'https://open.bigmodel.cn/api/anthropic',
      openaiChatCompletions: 'https://open.bigmodel.cn/api/paas/v4',
    },
  },
  {
    id: 'zhipu-zai',
    labelKey: 'routes.pool.page.apiVendorZhipuZai',
    endpoints: {
      claudeMessages: 'https://api.z.ai/api/anthropic',
      openaiChatCompletions: 'https://api.z.ai/api/paas/v4',
    },
  },
  {
    id: 'kimi-cn',
    labelKey: 'routes.pool.page.apiVendorKimiCn',
    endpoints: {
      claudeMessages: 'https://api.moonshot.cn/anthropic',
      openaiChatCompletions: 'https://api.moonshot.cn/v1',
    },
  },
  {
    id: 'kimi-global',
    labelKey: 'routes.pool.page.apiVendorKimiGlobal',
    endpoints: {
      claudeMessages: 'https://api.moonshot.ai/anthropic',
      openaiChatCompletions: 'https://api.moonshot.ai/v1',
    },
  },
  {
    id: 'openrouter',
    labelKey: 'routes.pool.page.apiVendorOpenRouter',
    endpoints: {
      claudeMessages: 'https://openrouter.ai/api/v1',
      openaiResponses: 'https://openrouter.ai/api/v1',
      grokResponses: 'https://openrouter.ai/api/v1',
      openaiChatCompletions: 'https://openrouter.ai/api/v1',
    },
  },
  {
    id: 'nvidia',
    labelKey: 'routes.pool.page.apiVendorNvidia',
    endpoints: { openaiChatCompletions: 'https://integrate.api.nvidia.com/v1' },
  },
  {
    id: 'groq',
    labelKey: 'routes.pool.page.apiVendorGroq',
    endpoints: { openaiChatCompletions: 'https://api.groq.com/openai/v1' },
  },
  {
    id: 'gemini',
    labelKey: 'routes.pool.page.apiVendorGemini',
    endpoints: { openaiChatCompletions: 'https://generativelanguage.googleapis.com/v1beta/openai/' },
  },
  {
    id: 'mistral',
    labelKey: 'routes.pool.page.apiVendorMistral',
    endpoints: { openaiChatCompletions: 'https://api.mistral.ai/v1' },
  },
];

/** Dropdown id for unknown / custom vendors. Always listed first. */
export const CUSTOM_VENDOR_ID = 'custom';

/** Known vendors sorted by the label shown in the picker. */
export function sortApiVendorsForPicker<T>(
  vendors: readonly T[],
  labelOf: (vendor: T) => string,
  locale?: string,
): T[] {
  return [...vendors].sort((left, right) =>
    labelOf(left).localeCompare(labelOf(right), locale, {
      numeric: true,
      sensitivity: 'base',
    }),
  );
}

export function poolApiChoices(agents: readonly AgentId[]): PoolApiChoice[] {
  return API_CHOICES.map((choice) => ({
    ...choice,
    available: agents.includes(choice.agentId),
  }));
}

export function poolSurfaceForApiChoice(
  choice: Pick<PoolApiChoice, 'endpoint'>,
): RoutePoolSurface {
  if (choice.endpoint === '/v1/messages') return 'messages';
  if (choice.endpoint === '/v1/chat/completions') return 'chat_completions';
  return 'responses';
}

export function normalizeApiBaseUrl(url: string): string {
  return url.trim().replace(/\/+$/, '');
}

function hostnameOf(url: string): string | null {
  try {
    return new URL(normalizeApiBaseUrl(url)).hostname.toLowerCase();
  } catch {
    return null;
  }
}

function vendorEndpointUrls(vendor: ApiVendorPreset): string[] {
  return Object.values(vendor.endpoints).filter((value): value is string => Boolean(value));
}

/** Match a pasted service URL to a known vendor, by exact URL then by host. */
export function matchApiVendor(url: string): ApiVendorPreset | null {
  const normalized = normalizeApiBaseUrl(url).toLowerCase();
  if (!normalized) return null;
  for (const vendor of API_VENDORS) {
    for (const endpointUrl of vendorEndpointUrls(vendor)) {
      if (normalizeApiBaseUrl(endpointUrl).toLowerCase() === normalized) return vendor;
    }
  }
  const host = hostnameOf(url);
  if (!host) return null;
  const byHost = API_VENDORS.filter((vendor) =>
    vendorEndpointUrls(vendor).some((endpointUrl) => hostnameOf(endpointUrl) === host),
  );
  return byHost.length === 1 ? byHost[0] : null;
}

export function primaryVendorUrl(vendor: ApiVendorPreset): string {
  return vendorServiceUrls(vendor)[0] ?? '';
}

/** Distinct service URLs for a vendor, in the same order as the URL field options. */
export function vendorServiceUrls(vendor: ApiVendorPreset): string[] {
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const type of PRIMARY_ENDPOINT_ORDER) {
    const url = vendor.endpoints[type];
    if (!url) continue;
    const normalized = normalizeApiBaseUrl(url).toLowerCase();
    if (seen.has(normalized)) continue;
    seen.add(normalized);
    urls.push(url);
  }
  return urls;
}

export function vendorEndpointTypes(vendor: ApiVendorPreset): PoolApiChoiceType[] {
  return PRIMARY_ENDPOINT_ORDER.filter((type) => Boolean(vendor.endpoints[type]));
}

export function resolveEndpointUrl(
  vendor: ApiVendorPreset | null,
  type: PoolApiChoiceType,
  fallbackUrl: string,
): string {
  return vendor?.endpoints[type] ?? normalizeApiBaseUrl(fallbackUrl);
}

export function defaultSelectedApiTypes(
  vendor: ApiVendorPreset | null,
  choices: readonly PoolApiChoice[],
): Set<PoolApiChoiceType> {
  if (!vendor) return new Set();
  const supported = new Set(vendorEndpointTypes(vendor));
  return new Set(
    choices
      .filter((choice) => choice.available && supported.has(choice.type))
      .map((choice) => choice.type),
  );
}

export function detectedApiChoiceTypes(
  types: readonly DetectedApiEndpointType[],
): PoolApiChoiceType[] {
  return types.flatMap((type) => DETECTED_API_CHOICES[type] ?? []);
}

export type PoolApiSaveItem = {
  choice: PoolApiChoice;
  baseUrl: string;
};

export function buildPoolApiSaveItems(
  choices: readonly PoolApiChoice[],
  selectedTypes: ReadonlySet<PoolApiChoiceType>,
  vendor: ApiVendorPreset | null,
  enteredUrl: string,
): PoolApiSaveItem[] {
  return choices
    .filter((choice) => choice.available && selectedTypes.has(choice.type))
    .map((choice) => ({
      choice,
      baseUrl: resolveEndpointUrl(vendor, choice.type, enteredUrl),
    }))
    .filter((item) => item.baseUrl.length > 0);
}

export function poolApiRecordName(baseUrl: string, endpoint: PoolApiChoice['endpoint']): string {
  try {
    return `${new URL(baseUrl).host} ${endpoint}`;
  } catch {
    return endpoint;
  }
}

/** One API key per line. Empty lines dropped; first occurrence wins. */
export function parseApiKeyLines(raw: string): string[] {
  const seen = new Set<string>();
  const keys: string[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const key = line.trim();
    if (!key || seen.has(key)) continue;
    seen.add(key);
    keys.push(key);
  }
  return keys;
}

/** Digits only. Empty input is unset. */
export function parsePriorityInput(raw: string): number | null {
  const text = raw.trim();
  if (!text) return null;
  if (!/^\d+$/.test(text)) return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

/** One exclusion rule per line or comma. Wildcards stay as written. */
export function parseExcludedModelRules(raw: string): string[] {
  const seen = new Set<string>();
  const rules: string[] = [];
  for (const part of raw.split(/[\n,]/)) {
    const rule = part.trim();
    const key = rule.toLowerCase();
    if (!rule || seen.has(key)) continue;
    seen.add(key);
    rules.push(rule);
  }
  return rules;
}

function wildcardPattern(rule: string): RegExp {
  return new RegExp(
    `^${rule.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*')}$`,
    'i',
  );
}

export function modelMatchesRule(modelId: string, rule: string): boolean {
  const normalized = rule.trim();
  if (!normalized) return false;
  return wildcardPattern(normalized).test(modelId.trim());
}

export function filterModelsByExclusions(
  models: readonly string[],
  rules: readonly string[],
): string[] {
  if (rules.length === 0) {
    return [...new Set(models.map((model) => model.trim()).filter(Boolean))];
  }
  const seen = new Set<string>();
  const out: string[] = [];
  for (const model of models) {
    const id = model.trim();
    if (!id || seen.has(id)) continue;
    if (rules.some((rule) => modelMatchesRule(id, rule))) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

/** Keep custom names, then append fetched names that are not excluded. */
export function mergeFetchedModels(
  current: readonly string[],
  fetched: readonly string[],
  excludedRules: readonly string[],
): string[] {
  const fetchedSet = new Set(fetched.map((model) => model.trim()).filter(Boolean));
  const custom = current.filter((model) => {
    const id = model.trim();
    return id.length > 0 && !fetchedSet.has(id);
  });
  return [...custom, ...filterModelsByExclusions(fetched, excludedRules)];
}
