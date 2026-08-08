#!/usr/bin/env node
/**
 * Sync crates/agenthub-core/src/usage/embedded-pricing.json from LiteLLM.
 *
 * Strategy (ccusage-inspired, offline-first):
 * 1. Fetch LiteLLM model_prices_and_context_window.json
 * 2. Keep only AgentHub-relevant, first-party-ish keys (no Azure/Bedrock/OpenRouter mirrors)
 * 3. Convert per-token USD → per-1M USD (pricing table unit)
 * 4. Add short aliases (date strip, 4-5 → 4.5, xai/grok-4 → grok-4)
 * 5. Overlay scripts/pricing/overrides.json (local models always win)
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
 * Keep rows useful for Claude / Codex / Kimi / Grok / Pi-style model ids.
 * Applied to full key and bare segment after last '/'.
 */
function isRelevantKey(key) {
  if (EXCLUDE_KEY.test(key) || EXCLUDE_AWS_STYLE.test(key) || EXCLUDE_ANTHROPIC_DOT.test(key)) {
    return false;
  }
  if (EXCLUDE_SUFFIX.test(key)) return false;
  const bare = bareName(key);
  if (EXCLUDE_SUFFIX.test(bare)) return false;

  return (
    /^(claude-|gpt-4|gpt-5|o[1-4]|codex)/i.test(bare) ||
    /^(claude-|gpt-4|gpt-5|o[1-4]|codex)/i.test(key) ||
    /^moonshot\//i.test(key) ||
    /^xai\/grok/i.test(key) ||
    /^kimi-/i.test(bare) ||
    /^grok-/i.test(bare)
  );
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
 * @returns {{ input: number, output: number, cacheCreate?: number, cacheRead?: number } | null}
 */
function rowFromLiteLLM(entry) {
  if (!entry || typeof entry !== 'object') return null;
  const input = perTokenToPerMillion(entry.input_cost_per_token);
  const output = perTokenToPerMillion(entry.output_cost_per_token);
  if (input == null || output == null) return null;
  if (input === 0 && output === 0) return null;

  /** @type {{ input: number, output: number, cacheCreate?: number, cacheRead?: number }} */
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
  return row;
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
 */
function buildTable(litellm, overridesRaw) {
  /** @type {Record<string, { input: number, output: number, cacheCreate?: number, cacheRead?: number }>} */
  const table = {};
  let fromLitellm = 0;
  const aliases = [];

  for (const [key, entry] of Object.entries(litellm)) {
    if (key === 'sample_spec') continue;
    if (!isRelevantKey(key)) continue;
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

  // overrides win
  /** @type {Record<string, { input: number, output: number, cacheCreate?: number, cacheRead?: number }>} */
  const overrides = {};
  for (const [k, v] of Object.entries(overridesRaw)) {
    if (k.startsWith('$')) continue;
    if (!v || typeof v !== 'object') continue;
    const input = Number(v.input);
    const output = Number(v.output);
    if (!Number.isFinite(input) || !Number.isFinite(output)) continue;
    /** @type {{ input: number, output: number, cacheCreate?: number, cacheRead?: number }} */
    const row = { input, output };
    if (v.cacheCreate != null && Number.isFinite(Number(v.cacheCreate))) {
      row.cacheCreate = Number(v.cacheCreate);
    }
    if (v.cacheRead != null && Number.isFinite(Number(v.cacheRead))) {
      row.cacheRead = Number(v.cacheRead);
    }
    overrides[k] = row;
    table[k] = row;
  }

  // Required smoke keys (must exist after build for AgentHub agents).
  const required = [
    'claude-sonnet-4',
    'claude-opus-4',
    'claude-opus-5',
    'gpt-5',
    'gpt-4o',
    'o4-mini',
    'moonshot/kimi-k2.5',
    'grok-4',
    'kimi-for-coding',
    'codex-auto-review',
  ];
  const missingRequired = required.filter((k) => !table[k]);
  if (missingRequired.length) {
    throw new Error(
      `pricing build missing required keys: ${missingRequired.join(', ')}. ` +
        `Add overrides or fix include filters.`,
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
  const litellm = await fetchLiteLLM();
  if (!litellm || typeof litellm !== 'object') {
    throw new Error('LiteLLM response is not an object');
  }

  const { table, fromLitellm, aliasCount, overrideCount } = buildTable(litellm, overrides);
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
