import type { UsagePort } from '@/lib/backend/contracts';
import type { UsageOverview, UsageQuery } from '@/lib/backend/contracts/usage-types';
import { delay } from '@/dev/mocks/delay';
import { isCapabilityUsable } from '@/lib/capability';
import type { AgentId, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';
import { denseTrendBuckets, localTrendBucket, trendGrain } from '@/lib/usage-trend';
import { usageTokenParts } from '@/lib/usage-tokens';
import { MOCK_CAPABILITIES } from './capabilities';

/**
 * Demo agents with seeded usage. Fixed list — do not use runtime AGENT_IDS here:
 * module init runs before catalog seed, so AGENT_IDS may still be empty.
 */
const DEMO_USAGE_AGENTS: AgentId[] = ['claude', 'codex', 'kimi', 'grok'];

const MODELS: Partial<Record<AgentId, string[]>> = {
  claude: ['claude-opus-4.5', 'claude-sonnet-4.5', 'claude-haiku-4'],
  codex: ['gpt-5.1-codex', 'gpt-5.1-codex-mini'],
  kimi: ['kimi-k2', 'kimi-k2-turbo'],
  grok: ['grok-code-fast-1'],
};

function usageCapable(agentId: AgentId): boolean {
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
  if (!ignoreModel && q.model && q.model !== 'all' && r.model !== q.model) return false;
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
    const key = groupByAgent ? r.agentId : r.model;
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
        .map((r) => r.model)
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
        .sort((a, b) => b.timestamp.localeCompare(a.timestamp));
      return q.limit != null ? filtered.slice(0, q.limit) : filtered;
    },

    async usageOverview(q) {
      await delay(30 + Math.random() * 50);
      return mockUsageOverview(q);
    },

    async usageTrend(days, agentId, model, since, excludeAgentIds) {
      await delay(30 + Math.random() * 50);
      const grain = trendGrain(days);
      const emptyPoint = (key: string): UsageTrendPoint => {
        const point: UsageTrendPoint = { date: key };
        for (const a of DEMO_USAGE_AGENTS) point[a] = 0;
        return point;
      };
      const byBucket = new Map<string, UsageTrendPoint>();
      for (const r of records) {
        if (!inUsageWindow(r, days, since)) continue;
        if (agentId && agentId !== 'all' && r.agentId !== agentId) continue;
        if (excludeAgentIds?.includes(r.agentId)) continue;
        if (model && model !== 'all' && r.model !== model) continue;
        const key = localTrendBucket(r.timestamp, grain);
        if (!key) continue;
        if (!byBucket.has(key)) byBucket.set(key, emptyPoint(key));
        const point = byBucket.get(key)!;
        const p = usageTokenParts(r);
        point[r.agentId] =
          (point[r.agentId] as number) + p.billableInput + p.cache + r.outputTokens;
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
      return [...new Set(records.map((r) => r.model))];
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
  };
}
