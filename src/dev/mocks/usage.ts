import type { UsagePort } from '@/lib/backend/contracts';
import { delay } from '@/dev/mocks/delay';
import { isCapabilityUsable } from '@/lib/capability';
import type { AgentId, ParserHealth, UsageRecord, UsageTrendPoint } from '@/lib/types';
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

const rand = seededRandom(20260726);

const records: UsageRecord[] = (() => {
  const out: UsageRecord[] = [];
  const now = new Date();
  let id = 0;
  for (let d = 29; d >= 0; d--) {
    const day = new Date(now.getTime() - d * 24 * 3600 * 1000);
    for (const agentId of DEMO_USAGE_AGENTS) {
      if (!usageCapable(agentId)) continue;
      const sessions = Math.floor(rand() * 6) + (d < 7 ? 2 : 0);
      for (let s = 0; s < sessions; s++) {
        const models = MODELS[agentId];
        if (!models?.length) continue;
        const input = Math.floor(rand() * 80000) + 2000;
        const output = Math.floor(rand() * 20000) + 500;
        const cacheRead = Math.floor(input * rand() * 0.8);
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
          costUsd: Math.round((input * 0.000015 + output * 0.000075) * 100) / 100,
          sessionId: `${agentId}-${day.toISOString().slice(0, 10)}-${s}`,
        });
      }
    }
  }
  return out;
})();

export function createMockUsagePort(): UsagePort {
  return {
    async getAvailability() {
      return { status: 'available' as const };
    },

    async queryUsage(q) {
      await delay(200 + Math.random() * 400);
      const cutoff = Date.now() - q.days * 24 * 3600 * 1000;
      return records.filter(
        (r) =>
          new Date(r.timestamp).getTime() >= cutoff &&
          (q.agentId === 'all' || !q.agentId || r.agentId === q.agentId) &&
          (q.model === 'all' || !q.model || r.model === q.model),
      );
    },

    async usageTrend(days, agentId) {
      await delay(200);
      const cutoff = Date.now() - days * 24 * 3600 * 1000;
      const byDay = new Map<string, UsageTrendPoint>();
      for (const r of records) {
        const t = new Date(r.timestamp).getTime();
        if (t < cutoff) continue;
        if (agentId && agentId !== 'all' && r.agentId !== agentId) continue;
        const day = r.timestamp.slice(0, 10);
        if (!byDay.has(day)) {
          const point: UsageTrendPoint = { date: day };
          for (const a of DEMO_USAGE_AGENTS) point[a] = 0;
          byDay.set(day, point);
        }
        const point = byDay.get(day)!;
        point[r.agentId] = (point[r.agentId] as number) + r.inputTokens + r.outputTokens;
      }
      return [...byDay.values()].sort((a, b) => a.date.localeCompare(b.date));
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
