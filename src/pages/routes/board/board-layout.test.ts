import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { flattenKeys } from '@/lib/i18n';
import { en } from '@/lib/i18n/locales/en';
import { zh } from '@/lib/i18n/locales/zh';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('routes board layout wiring', () => {
  it('keeps fleet health on the page and usage charts in the usage section', () => {
    const page = source('index.tsx');
    expect(page).toContain('BoardUsageSection');
    expect(page).toContain('boardFleetSummary');
    expect(page).toContain('partitionBoardRows');
    expect(page).not.toContain('recharts');
  });

  it('plots overlay usage series in resolved hex, not a stacked CSS-var area', () => {
    const section = source('board-usage-section.tsx');
    expect(section).toContain('resolveChartColor');
    expect(section).toContain('type="monotone"');
    expect(section).toContain('isAnimationActive={false}');
    expect(section).toContain('board-usage-fill-');
    expect(section).not.toContain('stackId');
    expect(section).not.toContain('type="linear"');
    expect(section).not.toContain('stroke={meta.color}');
    expect(section).not.toContain('stopColor={meta.color}');
  });

  it('groups local-forwarding usage by entry, model, and endpoint, not by Agent session logs', () => {
    const section = source('board-usage-section.tsx');
    expect(source('use-board-usage.ts')).toContain('gatewayUsageQuery');
    expect(section).toContain('routes.board.allEntries');
    expect(section).toContain('routes.board.allSurfaces');
    expect(section).toContain('dashboard.page.allModels');
    expect(section).toContain('SegmentedControl');
    expect(section).toContain('routes.board.groupByAria');
    expect(section).toContain('availableBoardGroupBy');
    expect(section).toContain('coerceBoardGroupBy');
    expect(section).not.toContain('dashboard.page.allAgents');
    expect(section).not.toContain('dashboard.page.distByAgent');
    expect(section).toContain('rememberedBoardUsageFilters');
    expect(section).toContain('rememberBoardUsageFilters');
  });

  it('does not drop route start/stop while adding charts', () => {
    const page = source('index.tsx');
    expect(page).toContain('handleStartBridge');
    expect(page).toContain('setStopConfirm');
    expect(page).toContain('BoardRouteRow');
  });
});

const BANNED_UI = /票|钱包|投影|真源|PKCE|loopback|\bTicket\b|\bwallet\b|\bAdapter\b|\bwire/i;

function lookup(obj: unknown, key: string): string {
  let cur: unknown = obj;
  for (const part of key.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return key;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : key;
}

describe('routes board user-facing copy', () => {
  it('keeps board copy free of banned jargon', () => {
    const keys = flattenKeys(zh).filter((key) => key.startsWith('routes.board.'));
    expect(keys.length).toBeGreaterThan(10);
    for (const key of keys) {
      expect(lookup(zh, key), key).not.toMatch(BANNED_UI);
      expect(lookup(en, key), key).not.toMatch(BANNED_UI);
    }
  });
});
