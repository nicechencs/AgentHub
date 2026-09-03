import type { UsagePort } from '@/lib/backend/contracts';
import type {
  GatewayUsageOverview,
  GatewayUsageQuery,
  GatewayUsageRow,
  UsageOverview,
  UsageQuery,
} from '@/lib/backend/contracts/usage-types';
import { delay } from '@/dev/mocks/delay';
import { isCapabilityUsable } from '@/lib/capability';
import type { AgentKey, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { denseTrendBuckets, localTrendBucket, trendGrain } from '@/lib/usage-trend';
import { canonicalUsageModel, usageModelsMatch } from '@/lib/usage-model';
import { usageTokenParts } from '@/lib/usage-tokens';
import { listMockAdapterProfiles } from './adapter';
import { MOCK_CAPABILITIES } from './capabilities';

/**
 * Demo agents with seeded usage. Fixed list — do not use runtime AGENT_IDS here:
 * module init runs before catalog seed, so AGENT_IDS may still be empty.
 */
const DEMO_USAGE_AGENTS: AgentKey[] = ['claude', 'codex', 'kimi', 'grok'];

const MODELS: Partial<Record<AgentKey, string[]>> = {
  claude: ['claude-opus-4.5', 'claude-sonnet-4.5', 'claude-haiku-4'],
  codex: ['gpt-5.1-codex', 'gpt-5.1-codex-mini'],
  kimi: ['kimi-k2', 'kimi-k2-turbo'],
  grok: ['grok-code-fast-1'],
};

function usageCapable(agentId: AgentKey): boolean {
  const cap = MOCK_CAPABILITIES[agentId]?.usage;
  return isCapabilityUsable(cap) || (cap?.level === 'planned' && !!MODELS[agentId]?.length);
}

function seededRandom(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

function buildRecords(nowMs: number): UsageRecord[] {
  const rand = seededRandom(20260726);
  const out: UsageRecord[] = [];
  let id = 0;
  for (let d = 29; d >= 0; d--) {
    const day = new Date(nowMs - d * 24 * 3600 * 1000);
    for (const agentId of DEMO_USAGE_AGENTS) {
      if (!usageCapable(agentId)) continue;
      const sessions = Math.floor(rand() * 6) + (d < 7 ? 2 : 0);
      for (let s = 0; s < sessions; s++) {
        const models = MODELS[agentId];
        if (!models?.length) continue;
        const input = Math.floor(rand() * 80000) + 2000;
        const output = Math.floor(rand() * 20000) + 500;
        const cacheRead = Math.floor(input * rand() * 0.8);
        const cacheWrite = Math.floor(input * rand() * 0.15);
        const ts = new Date(day);
        ts.setHours(Math.floor(rand() * 14) + 8, Math.floor(rand() * 60), 0, 0);
        out.push({
          id: `u-${id++}`,
          timestamp: ts.toISOString(),
          agentId,
          model: models[Math.floor(rand() * models.length)],
          inputTokens: input,
          outputTokens: output,
          cacheReadTokens: cacheRead,
          cacheWriteTokens: cacheWrite,
          costUsd: Math.round((input * 0.000015 + output * 0.000075) * 100) / 100,
          sessionId: `${agentId}-${day.toISOString().slice(0, 10)}-${s}`,
        });
      }
    }
  }
  return out;
}

let records = buildRecords(Date.now());

/** Regenerates the 30-day usage window so each backend factory starts relative to now. */
export function resetMockUsage(): void {
  records = buildRecords(Date.now());
}

function inUsageWindow(r: UsageRecord, days: number, since?: string): boolean {
  const t = new Date(r.timestamp).getTime();
  const cutoff = Date.now() - days * 24 * 3600 * 1000;
  if (t < cutoff) return false;
  if (since) {
    const bound = new Date(since).getTime();
    if (!Number.isNaN(bound) && t < bound) return false;
  }
  return true;
}

function matchesUsageQuery(r: UsageRecord, q: UsageQuery, ignoreModel = false): boolean {
  if (!inUsageWindow(r, q.days, q.since)) return false;
  if (q.agentId && q.agentId !== 'all' && r.agentId !== q.agentId) return false;
  if (q.excludeAgentIds?.includes(r.agentId)) return false;
  if (!ignoreModel && q.model && q.model !== 'all' && !usageModelsMatch(r.model, q.model)) {
    return false;
  }
  return true;
}

function mockUsageOverview(q: UsageQuery): UsageOverview {
  const rows = records.filter((r) => matchesUsageQuery(r, q));
  let billableInput = 0;
  let output = 0;
  let cacheRead = 0;
  let cacheWrite = 0;
  let costUsd = 0;
  const byKey = new Map<
    string,
    {
      key: string;
      tokens: number;
      costUsd: number;
      billableInput: number;
      output: number;
      cacheRead: number;
      cacheWrite: number;
    }
  >();
  const groupByAgent = !q.agentId || q.agentId === 'all';
  for (const r of rows) {
    const p = usageTokenParts(r);
    billableInput += p.billableInput;
    output += r.outputTokens;
    cacheRead += p.cacheRead;
    cacheWrite += p.cacheWrite;
    costUsd += r.costUsd;
    const key = groupByAgent ? r.agentId : canonicalUsageModel(r.model) || r.model;
    const entry = byKey.get(key) ?? {
      key,
      tokens: 0,
      costUsd: 0,
      billableInput: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
    };
    entry.tokens += p.billableInput + p.cache + r.outputTokens;
    entry.costUsd += r.costUsd;
    entry.billableInput += p.billableInput;
    entry.output += r.outputTokens;
    entry.cacheRead += p.cacheRead;
    entry.cacheWrite += p.cacheWrite;
    byKey.set(key, entry);
  }
  const models = [
    ...new Set(
      records
        .filter((r) => matchesUsageQuery(r, q, true))
        .map((r) => canonicalUsageModel(r.model) || r.model)
        .filter((m) => m.length > 0),
    ),
  ].sort((a, b) => a.localeCompare(b));
  return {
    metrics: { billableInput, output, cacheRead, cacheWrite, costUsd },
    distribution: [...byKey.values()].sort((a, b) => b.tokens - a.tokens),
    models,
  };
}

export function createMockUsagePort(): UsagePort {
  return {
    async getAvailability() {
      return { status: 'available' as const };
    },

    async queryUsage(q) {
      await delay(200 + Math.random() * 400);
      const filtered = records
        .filter((r) => matchesUsageQuery(r, q))
        .sort((a, b) => b.timestamp.localeCompare(a.timestamp))
        .map((r) => {
          const model = canonicalUsageModel(r.model);
          return model && model !== r.model ? { ...r, model } : r;
        });
      return q.limit != null ? filtered.slice(0, q.limit) : filtered;
    },

    async usageOverview(q) {
      await delay(30 + Math.random() * 50);
      return mockUsageOverview(q);
    },

    async usageTrend(days, agentId, model, since, excludeAgentIds, groupBy) {
      await delay(30 + Math.random() * 50);
      const grain = trendGrain(days);
      const byModel = groupBy === 'model';
      const emptyPoint = (key: string): UsageTrendPoint => {
        const point: UsageTrendPoint = { date: key };
        if (!byModel) {
          for (const a of DEMO_USAGE_AGENTS) point[a] = 0;
        }
        return point;
      };
      const byBucket = new Map<string, UsageTrendPoint>();
      for (const r of records) {
        if (!inUsageWindow(r, days, since)) continue;
        if (agentId && agentId !== 'all' && r.agentId !== agentId) continue;
        if (excludeAgentIds?.includes(r.agentId)) continue;
        if (model && model !== 'all' && !usageModelsMatch(r.model, model)) continue;
        const key = localTrendBucket(r.timestamp, grain);
        if (!key) continue;
        if (!byBucket.has(key)) byBucket.set(key, emptyPoint(key));
        const point = byBucket.get(key)!;
        const p = usageTokenParts(r);
        const tokens = p.billableInput + p.cache + r.outputTokens;
        if (byModel) {
          const series = canonicalUsageModel(r.model) || r.model;
          if (!series) continue;
          point[series] = (Number(point[series]) || 0) + tokens;
          const costKey = `__cost__:${series}`;
          point[costKey] = (Number(point[costKey]) || 0) + r.costUsd;
        } else {
          point[r.agentId] = (point[r.agentId] as number) + tokens;
        }
      }
      if (byBucket.size > 0) {
        for (const key of denseTrendBuckets(days, since)) {
          if (!byBucket.has(key)) byBucket.set(key, emptyPoint(key));
        }
      }
      return [...byBucket.values()].sort((a, b) => a.date.localeCompare(b.date));
    },

    async listModels() {
      await delay(100);
      return [
        ...new Set(
          records
            .map((r) => canonicalUsageModel(r.model) || r.model)
            .filter((m) => m.length > 0),
        ),
      ].sort((a, b) => a.localeCompare(b));
    },

    async parserHealth(): Promise<ParserHealth[]> {
      await delay(200);
      return [
        { agentId: 'claude', supported: true, records: 5231 },
        { agentId: 'codex', supported: true, records: 1104 },
        { agentId: 'kimi', supported: true, records: 250, failRatePct: 12, skipped: 34 },
        { agentId: 'grok', supported: true, records: 88 },
      ];
    },

    async missingPricingModels() {
      await delay(50);
      return [] as string[];
    },

    async collectUsage(onProgress) {
      const steps = 8;
      for (let i = 1; i <= steps; i++) {
        await delay(250 + Math.random() * 200);
        onProgress?.(Math.round((i / steps) * 100));
      }
      return {
        inserted: 0,
        skipped: 0,
        failed: 0,
        agents: [],
        missingPricingModels: [],
      };
    },

    async gatewayUsageQuery(q = {}) {
      await delay(30 + Math.random() * 50);
      return filterMockGatewayRows(q);
    },

    async gatewayUsageOverview(q = {}) {
      await delay(30 + Math.random() * 50);
      return summarizeMockGatewayRows(filterMockGatewayRows(q));
    },
  };
}

const GATEWAY_SURFACES = ['messages', 'responses', 'chat'] as const;

function defaultSurfaceForAgent(agentId: string): (typeof GATEWAY_SURFACES)[number] {
  if (agentId === 'claude') return 'messages';
  if (agentId === 'grok') return 'chat';
  return 'responses';
}

function buildMockGatewayRows(nowMs: number): GatewayUsageRow[] {
  const profiles = listMockAdapterProfiles().filter((profile) => profile.route === 'local_bridge');
  if (profiles.length === 0) return [];
  const rand = seededRandom(20260831);
  const out: GatewayUsageRow[] = [];
  let n = 0;
  for (let d = 29; d >= 0; d--) {
    const day = new Date(nowMs - d * 24 * 3600 * 1000);
    for (const profile of profiles) {
      const models = MODELS[profile.targetAgentId] ?? ['gpt-4.1'];
      const hits = Math.floor(rand() * 5) + (d < 7 ? 2 : 1);
      for (let i = 0; i < hits; i++) {
        const ts = new Date(day);
        ts.setHours(Math.floor(rand() * 14) + 8, Math.floor(rand() * 60), 0, 0);
        const failed = rand() < 0.08;
        const surface =
          rand() < 0.55 ? defaultSurfaceForAgent(profile.targetAgentId) : GATEWAY_SURFACES[Math.floor(rand() * 3)];
        const input = Math.floor(rand() * 40000) + 800;
        const output = Math.floor(rand() * 12000) + 200;
        const cached = Math.floor(input * rand() * 0.4);
        out.push({
          requestId: `g-${n++}`,
          ts: ts.toISOString(),
          profileId: profile.id,
          surface,
          model: models[Math.floor(rand() * models.length)],
          inputTokens: input,
          outputTokens: failed ? 0 : output,
          cachedInputTokens: cached,
          status: failed ? 'failed' : 'ok',
          latencyMs: Math.floor(rand() * 1800) + 80,
        });
      }
    }
  }
  return out;
}

function filterMockGatewayRows(q: GatewayUsageQuery): GatewayUsageRow[] {
  const rows = buildMockGatewayRows(Date.now());
  const since = q.since ?? null;
  const until = q.until ?? null;
  const profileId = q.profileId ?? null;
  const filtered = rows.filter((row) => {
    if (since && row.ts < since) return false;
    if (until && row.ts > until) return false;
    if (profileId && row.profileId !== profileId) return false;
    return true;
  });
  filtered.sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0));
  return q.limit != null ? filtered.slice(0, q.limit) : filtered;
}

function summarizeMockGatewayRows(rows: readonly GatewayUsageRow[]): GatewayUsageOverview {
  let okCount = 0;
  let failedCount = 0;
  let inputTokens = 0;
  let outputTokens = 0;
  let cachedInputTokens = 0;
  let reasoningTokens = 0;
  let latencySum = 0;
  let latencyN = 0;
  let ttftSum = 0;
  let ttftN = 0;
  const latencies: number[] = [];
  for (const row of rows) {
    if (row.status === 'ok') okCount += 1;
    else failedCount += 1;
    inputTokens += row.inputTokens;
    outputTokens += row.outputTokens;
    cachedInputTokens += row.cachedInputTokens ?? 0;
    reasoningTokens += row.reasoningTokens ?? 0;
    if (typeof row.latencyMs === 'number') {
      latencySum += row.latencyMs;
      latencyN += 1;
      latencies.push(row.latencyMs);
    }
    if (typeof row.ttftMs === 'number') {
      ttftSum += row.ttftMs;
      ttftN += 1;
    }
  }
  latencies.sort((a, b) => a - b);
  const p95 = latencies.length === 0
    ? undefined
    : latencies[Math.max(0, Math.ceil(0.95 * latencies.length) - 1)];
  return {
    requestCount: rows.length,
    okCount,
    failedCount,
    inputTokens,
    outputTokens,
    cachedInputTokens,
    reasoningTokens,
    avgLatencyMs: latencyN > 0 ? latencySum / latencyN : undefined,
    p95LatencyMs: p95,
    avgTtftMs: ttftN > 0 ? ttftSum / ttftN : undefined,
  };
}
