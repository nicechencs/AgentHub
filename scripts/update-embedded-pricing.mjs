#!/usr/bin/env node
/**
 * Sync crates/agenthub-core/src/usage/embedded-pricing.json from LiteLLM.
 *
 * Strategy (ccusage-inspired, offline-first):
 * 1. Fetch LiteLLM model_prices_and_context_window.json
 * 2. Keep official publishers in scripts/pricing/vendors.json (not Agent families).
 *    Skip Azure/Bedrock/OpenRouter mirrors. dashscope is Qwen/QwQ only.
 * 3. Convert per-token USD → per-1M USD (pricing table unit)
 * 4. Add short aliases only from official rows (date strip, 4-5 → 4.5, xai/grok-4 → grok-4)
 * 5. Copy logAliases, then overlay scripts/pricing/overrides.json (always win)
 * 6. Write embedded table + meta; runtime never fetches pricing
 *
 * Usage:
 *   node scripts/update-embedded-pricing.mjs           # write files
 *   node scripts/update-embedded-pricing.mjs --check    # exit 1 if drift
 *   node scripts/update-embedded-pricing.mjs --dry-run  # print summary only
 */

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const OUT_JSON = join(ROOT, 'crates/agenthub-core/src/usage/embedded-pricing.json');
const OUT_META = join(ROOT, 'crates/agenthub-core/src/usage/embedded-pricing.meta.json');
const OVERRIDES_PATH = join(ROOT, 'scripts/pricing/overrides.json');
const REQUIRED_KEYS_PATH = join(ROOT, 'scripts/pricing/required-keys.json');
const VENDORS_PATH = join(ROOT, 'scripts/pricing/vendors.json');

const LITELLM_URL =
  process.env.LITELLM_PRICING_URL ??
  'https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json';

/** Hosted / regional mirrors we skip in favor of first-party catalog rows. */
const EXCLUDE_KEY =
  /^(azure|azure_ai|bedrock|bedrock_mantle|openrouter|github_copilot|databricks|vercel_ai_gateway|vertex_ai|fireworks_ai|deepinfra|cloudflare|replicate|perplexity|gmi|baseten|crusoe|groq|hyperbolic|novita|oci|tensormesh|together_ai|wandb)\//i;

const EXCLUDE_AWS_STYLE = /^(global|us|eu|au)\./i;
const EXCLUDE_ANTHROPIC_DOT = /^anthropic\./i;

/** Skip non-chat coding-agent noise. */
const EXCLUDE_SUFFIX =
  /(audio|realtime|tts|transcribe|diarize|search-preview|search-api|vision-preview|vision-beta)$/i;

/**
 * @typedef {{ providers: Set<string>, providerModelAllow: Record<string, string[]>, modes: Set<string>, logAliases: Record<string, string> }} VendorConfig
 */

function loadVendors() {
  const raw = JSON.parse(readFileSync(VENDORS_PATH, 'utf8'));
  if (!raw || typeof raw !== 'object') {
    throw new Error('scripts/pricing/vendors.json must be a JSON object');
  }
  if (!Array.isArray(raw.providers) || raw.providers.some((p) => typeof p !== 'string' || !p.trim())) {
    throw new Error('vendors.json providers must be a non-empty array of strings');
  }
  const providerModelAllow = {};
  const allowRaw = raw.providerModelAllow ?? {};
  if (typeof allowRaw !== 'object' || Array.isArray(allowRaw)) {
    throw new Error('vendors.json providerModelAllow must be an object');
  }
  for (const [provider, prefixes] of Object.entries(allowRaw)) {
    if (provider.startsWith('$')) continue;
    if (!Array.isArray(prefixes) || prefixes.some((p) => typeof p !== 'string' || !p.trim())) {
      throw new Error(`vendors.json providerModelAllow.${provider} must be an array of strings`);
    }
    providerModelAllow[provider.toLowerCase()] = prefixes.map((p) => p.toLowerCase());
  }
  const modesRaw = Array.isArray(raw.modes) && raw.modes.length ? raw.modes : ['chat', 'responses'];
  if (modesRaw.some((m) => typeof m !== 'string' || !m.trim())) {
    throw new Error('vendors.json modes must be an array of strings');
  }
  const logAliases = {};
  const aliasRaw = raw.logAliases ?? {};
  if (typeof aliasRaw !== 'object' || Array.isArray(aliasRaw)) {
    throw new Error('vendors.json logAliases must be an object');
  }
  for (const [from, to] of Object.entries(aliasRaw)) {
    if (from.startsWith('$')) continue;
    if (typeof from !== 'string' || typeof to !== 'string' || !from.trim() || !to.trim()) {
      throw new Error('vendors.json logAliases values must be non-empty strings');
    }
    logAliases[from] = to;
  }
  return {
    providers: new Set(raw.providers.map((p) => p.trim().toLowerCase())),
    providerModelAllow,
    modes: new Set(modesRaw.map((m) => m.trim())),
    logAliases,
  };
}

/** Official publisher row — not a reseller mirror, not an Agent-family name match. */
function isOfficialRow(key, entry, vendors) {
  if (!entry || typeof entry !== 'object') return false;
  if (EXCLUDE_KEY.test(key) || EXCLUDE_AWS_STYLE.test(key) || EXCLUDE_ANTHROPIC_DOT.test(key)) {
    return false;
  }
  if (EXCLUDE_SUFFIX.test(key)) return false;
  const bare = bareName(key);
  if (EXCLUDE_SUFFIX.test(bare)) return false;
  const mode = entry.mode;
  if (mode != null && mode !== '' && !vendors.modes.has(String(mode))) return false;
  const provider = String(entry.litellm_provider ?? '').trim().toLowerCase();
  if (vendors.providers.has(provider)) return true;
  const prefixes = vendors.providerModelAllow[provider];
  if (!prefixes || !prefixes.length) return false;
  const bareLower = bare.toLowerCase();
  return prefixes.some((prefix) => bareLower.startsWith(prefix));
}

function bareName(key) {
  const i = key.lastIndexOf('/');
  return i >= 0 ? key.slice(i + 1) : key;
}

function roundRate(n) {
  if (!Number.isFinite(n)) return null;
  // Keep enough precision for cheap cache-read rows without float noise.
  const r = Math.round(n * 1e6) / 1e6;
  return r;
}

function perTokenToPerMillion(v) {
  if (v == null || v === '') return null;
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n) || n < 0) return null;
  return roundRate(n * 1_000_000);
}

/**
 * @returns {Record<string, number> | null}
 */
function rowFromLiteLLM(entry) {
  if (!entry || typeof entry !== 'object') return null;
  const input = perTokenToPerMillion(entry.input_cost_per_token);
  const output = perTokenToPerMillion(entry.output_cost_per_token);
  if (input == null || output == null) return null;
  if (input === 0 && output === 0) return null;

  /** @type {Record<string, number>} */
  const row = { input, output };
  const cc =
    perTokenToPerMillion(entry.cache_creation_input_token_cost) ??
    perTokenToPerMillion(entry.cache_creation_input_token_cost_above_1hr) ??
    perTokenToPerMillion(entry.input_cost_per_token);
  const cr =
    perTokenToPerMillion(entry.cache_read_input_token_cost) ??
    perTokenToPerMillion(entry.cache_read_input_token_cost_above_1hr);
  if (cc != null) row.cacheCreate = cc;
  if (cr != null) row.cacheRead = cr;
  const ia = perTokenToPerMillion(entry.input_cost_per_token_above_200k_tokens);
  const oa = perTokenToPerMillion(entry.output_cost_per_token_above_200k_tokens);
  const cca = perTokenToPerMillion(entry.cache_creation_input_token_cost_above_200k_tokens);
  const cra = perTokenToPerMillion(entry.cache_read_input_token_cost_above_200k_tokens);
  if (ia != null) row.inputAbove200k = ia;
  if (oa != null) row.outputAbove200k = oa;
  if (cca != null) row.cacheCreateAbove200k = cca;
  if (cra != null) row.cacheReadAbove200k = cra;
  return row;
}

const OPTIONAL_RATE_FIELDS = [
  'cacheCreate',
  'cacheRead',
  'inputAbove200k',
  'outputAbove200k',
  'cacheCreateAbove200k',
  'cacheReadAbove200k',
  'longContextThreshold',
  'fastMultiplier',
];

function applyOptionalFields(row, src) {
  for (const field of OPTIONAL_RATE_FIELDS) {
    if (src[field] != null && Number.isFinite(Number(src[field]))) {
      row[field] = Number(src[field]);
    }
  }
}

function stableStringify(obj) {
  const keys = Object.keys(obj).sort((a, b) => a.localeCompare(b));
  /** @type {Record<string, unknown>} */
  const sorted = {};
  for (const k of keys) sorted[k] = obj[k];
  return `${JSON.stringify(sorted, null, 2)}\n`;
}

/**
 * Date / version tail peel for alias keys (align with pricing.rs strip_date_suffix spirit).
 * claude-sonnet-4-20250514 → claude-sonnet-4
 * claude-haiku-4-5-20251001 → claude-haiku-4-5
 */
function stripDateSuffix(id) {
  let cur = id;
  for (let i = 0; i < 3; i++) {
    const m = cur.match(/^(.*)-(\d{6,8})(?:-v[\d:]+)?$/);
    if (m) {
      cur = m[1];
      continue;
    }
    const m2 = cur.match(/^(.*)-v[\d:]+$/);
    if (m2) {
      cur = m2[1];
      continue;
    }
    break;
  }
  return cur === id ? null : cur;
}

/** claude-sonnet-4-5 → claude-sonnet-4.5 (log style). */
function dashVersionToDot(id) {
  // ...-4-5 or ...-4-5-xxx already stripped → ...-4.5
  const m = id.match(/^(.*?)-(\d+)-(\d+)$/);
  if (!m) return null;
  // avoid turning gpt-4-turbo into nonsense: only when last two segments are short version digits
  if (m[2].length > 2 || m[3].length > 2) return null;
  return `${m[1]}-${m[2]}.${m[3]}`;
}

function addAlias(table, key, row, aliases) {
  if (!key || table[key]) return;
  table[key] = row;
  aliases.push(key);
}

/**
 * Build pricing table from LiteLLM map + overrides.
 * @param {Record<string, unknown>} litellm
 * @param {Record<string, unknown>} overridesRaw
 * @param {VendorConfig} vendors
 */
function buildTable(litellm, overridesRaw, vendors) {
  /** @type {Record<string, { input: number, output: number, cacheCreate?: number, cacheRead?: number }>} */
  const table = {};
  let fromLitellm = 0;
  const aliases = [];

  for (const [key, entry] of Object.entries(litellm)) {
    if (key === 'sample_spec') continue;
    if (!isOfficialRow(key, entry, vendors)) continue;
    const row = rowFromLiteLLM(entry);
    if (!row) continue;

    // Prefer first-party key forms; skip if we already have exact key.
    if (!table[key]) {
      table[key] = row;
      fromLitellm += 1;
    }

    const bare = bareName(key);
    if (bare !== key) addAlias(table, bare, row, aliases);

    const stripped = stripDateSuffix(bare);
    if (stripped) {
      addAlias(table, stripped, row, aliases);
      const dotted = dashVersionToDot(stripped);
      if (dotted) addAlias(table, dotted, row, aliases);
    }
    const dottedBare = dashVersionToDot(bare);
    if (dottedBare) addAlias(table, dottedBare, row, aliases);

    // Family-friendly short ids used in AgentHub logs / UI.
    // e.g. claude-sonnet-4-20250514 → also ensure claude-sonnet-4 via strip
  }

  for (const [from, to] of Object.entries(vendors.logAliases)) {
    if (table[from]) continue;
    const target = table[to];
    if (!target) {
      throw new Error(
        `log alias ${from} → ${to} is missing the target row. ` +
          `Fix scripts/pricing/vendors.json or the include filters.`,
      );
    }
    addAlias(table, from, target, aliases);
  }

  // overrides win
  /** @type {Record<string, { input: number, output: number, cacheCreate?: number, cacheRead?: number }>} */
  const overrides = {};
  for (const [k, v] of Object.entries(overridesRaw)) {
    if (k.startsWith('$')) continue;
    if (!v || typeof v !== 'object') continue;
    const input = Number(v.input);
    const output = Number(v.output);
    if (!Number.isFinite(input) || !Number.isFinite(output)) continue;
    /** @type {Record<string, number>} */
    const row = { ...(table[k] ?? {}), input, output };
    applyOptionalFields(row, v);
    overrides[k] = row;
    table[k] = row;
  }

  // Required smoke keys live in scripts/pricing/required-keys.json (not generation filters).
  const required = JSON.parse(readFileSync(REQUIRED_KEYS_PATH, 'utf8'));
  if (!Array.isArray(required) || required.some((k) => typeof k !== 'string')) {
    throw new Error('scripts/pricing/required-keys.json must be a JSON array of strings');
  }
  const missingRequired = required.filter((k) => !table[k]);
  if (missingRequired.length) {
    throw new Error(
      `pricing build missing required keys: ${missingRequired.join(', ')}. ` +
        `Add overrides, logAliases, or fix vendor filters.`,
    );
  }

  return { table, fromLitellm, aliasCount: aliases.length, overrideCount: Object.keys(overrides).length };
}

function loadOverrides() {
  const raw = JSON.parse(readFileSync(OVERRIDES_PATH, 'utf8'));
  return raw;
}

async function fetchLiteLLM() {
  const res = await fetch(LITELLM_URL, {
    headers: { 'user-agent': 'agenthub-pricing-sync/1.0' },
  });
  if (!res.ok) {
    throw new Error(`LiteLLM fetch failed: HTTP ${res.status} ${res.statusText} (${LITELLM_URL})`);
  }
  return res.json();
}

function parseArgs(argv) {
  return {
    check: argv.includes('--check'),
    dryRun: argv.includes('--dry-run'),
  };
}

async function main() {
  const { check, dryRun } = parseArgs(process.argv.slice(2));
  const overrides = loadOverrides();
  const vendors = loadVendors();
  const litellm = await fetchLiteLLM();
  if (!litellm || typeof litellm !== 'object') {
    throw new Error('LiteLLM response is not an object');
  }

  const { table, fromLitellm, aliasCount, overrideCount } = buildTable(litellm, overrides, vendors);
  const body = stableStringify(table);
  const meta = {
    source: LITELLM_URL,
    fetchedAt: new Date().toISOString(),
    modelCount: Object.keys(table).length,
    fromLitellmRows: fromLitellm,
    aliasKeysAdded: aliasCount,
    overrideKeys: overrideCount,
    unit: 'USD per 1M tokens',
    notes:
      'Offline embedded snapshot. Runtime does not fetch pricing. Re-run scripts/update-embedded-pricing.mjs or wait for daily CI.',
  };
  const metaBody = `${JSON.stringify(meta, null, 2)}\n`;

  console.log(
    `[pricing] models=${meta.modelCount} litellmRows=${fromLitellm} aliases+=${aliasCount} overrides=${overrideCount}`,
  );

  if (dryRun) {
    console.log('[pricing] dry-run: not writing files');
    return;
  }

  if (check) {
    let current = '';
    try {
      current = readFileSync(OUT_JSON, 'utf8');
    } catch {
      current = '';
    }
    // Normalize both sides via re-parse for stable compare
    const currentNorm = current ? stableStringify(JSON.parse(current)) : '';
    if (currentNorm !== body) {
      console.error('[pricing] embedded-pricing.json is out of date. Run: pnpm pricing:update');
      process.exit(1);
    }
    console.log('[pricing] check ok — embedded table matches LiteLLM+overrides');
    return;
  }

  mkdirSync(dirname(OUT_JSON), { recursive: true });
  writeFileSync(OUT_JSON, body, 'utf8');
  writeFileSync(OUT_META, metaBody, 'utf8');
  console.log(`[pricing] wrote ${OUT_JSON}`);
  console.log(`[pricing] wrote ${OUT_META}`);
}

main().catch((err) => {
  console.error('[pricing]', err instanceof Error ? err.message : err);
  process.exit(1);
});
