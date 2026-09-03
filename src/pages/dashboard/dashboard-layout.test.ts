import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { flattenKeys, translate } from '@/lib/i18n';
import { en } from '@/lib/i18n/locales/en';
import { zh } from '@/lib/i18n/locales/zh';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('dashboard layout wiring', () => {
  it('folds the agent ready count into the page subtitle', () => {
    const page = source('index.tsx');
    expect(page).toContain('dashboardPageDescription');
    expect(page).toContain('description={pageDescription}');
    expect(page).not.toContain("description={t('dashboard.page.description')}");
  });

  it('applies the remembered Agent catalog order to overview cards', () => {
    const overview = source('AgentOverview.tsx');
    expect(overview).toContain('applyStoredAgentOrder');
    expect(overview).toContain('StorageKey.agentsCatalogOrder');
  });

  it('re-renders overview cards when the runtime catalog hydrates', () => {
    const overview = source('AgentOverview.tsx');
    expect(overview).toContain('useAgentCatalog');
    expect(overview).toContain('catalog.hydrated');
  });

  it('does not repeat Agent 总览 or a Manage button above the cards', () => {
    const overview = source('AgentOverview.tsx');
    expect(overview).not.toContain("t('dashboard.overview.title')");
    expect(overview).not.toContain("t('dashboard.overview.manage')");
    expect(overview).not.toContain("from '@/components/ui/button'");
    expect(source('index.tsx')).not.toContain("t('dashboard.overview.manage')");
  });

  it('restores usage filters from in-process memory when returning to the page', () => {
    const page = source('index.tsx');
    expect(page).toContain('rememberedUsageFilters');
    expect(page).toContain('rememberUsageFilters');
    expect(page).toContain('resolveUsageModelFilter');
    expect(page).not.toContain('formatUsageWindowLabel');
    expect(page).not.toContain('rounded-full border border-border bg-panel px-3 text-meta');
    expect(page).not.toContain("useState<DateRange>('7d')");
    expect(page).toContain('if (!modelsReady) return');
  });

  it('puts time range after agent and model filters and supports custom dates', () => {
    const page = source('index.tsx');
    const agentIdx = page.indexOf("t('dashboard.page.allAgents')");
    const modelIdx = page.indexOf("t('dashboard.page.allModels')");
    const rangeIdx = page.indexOf("t('dashboard.page.rangeAria')");
    expect(agentIdx).toBeGreaterThan(-1);
    expect(modelIdx).toBeGreaterThan(agentIdx);
    expect(rangeIdx).toBeGreaterThan(modelIdx);
    expect(page).toContain("'custom'");
    expect(page).toContain('type="date"');
    expect(page).toContain('dashboard.page.customStart');
  });

  it('plots agent series as overlay areas and model series as stacked cumulative fill', () => {
    const chart = source('UsageTrendChart.tsx');
    expect(chart).toContain('resolveChartColor');
    expect(chart).toContain('type="monotone"');
    expect(chart).toContain('isAnimationActive={false}');
    expect(chart).toContain('stackId="model-usage"');
    expect(chart).toContain('accumulateTrendSeries');
    expect(chart).toContain('foldTrendTail');
    expect(chart).toContain('ReferenceLine');
    expect(chart).not.toContain('<LineChart');
    expect(chart).not.toContain('type="linear"');
    expect(chart).not.toContain('stroke={meta.color}');
    expect(chart).not.toContain('stopColor={meta.color}');
  });

  it('switches the middle trend between Agent areas and stacked model usage with cost', () => {
    const page = source('index.tsx');
    const chart = source('UsageTrendChart.tsx');
    expect(page).toContain('UsageTrendChart');
    expect(page).toContain(
      "usageTrend(days, agentId, model, since, excludeAgentIds, 'model', until)",
    );
    expect(chart).toContain('SegmentedControl');
    expect(chart).toContain('dashboard.page.trendGroupModel');
    expect(chart).not.toContain('dashboard.page.trendMetricCost');
    expect(chart).toContain('fmtTrendCost');
    expect(chart).toContain('costFromTrendPoint');
    expect(chart).toContain('dashboard.page.trendOther');
    expect(chart).toContain('dashboard.page.trendToday');
  });

  it('keeps trend y-axis labels in view and sorts the hover list by tokens', () => {
    const page = source('index.tsx');
    const chart = source('UsageTrendChart.tsx');
    const tooltip = source('UsageTrendTooltip.tsx');
    expect(chart).toContain('USAGE_TREND_Y_AXIS_WIDTH');
    expect(chart).toContain('UsageTrendTooltipCard');
    expect(chart).toContain('useUsageTrendHover');
    expect(chart).not.toContain('width={48}');
    expect(chart).not.toContain('contentStyle={tooltipSurfaceStyle()}');
    expect(tooltip).toContain('USAGE_TREND_Y_AXIS_WIDTH = 64');
    expect(tooltip).toContain('tooltipSurfaceStyle()');
    expect(tooltip).toContain("pointerEvents: 'auto'");
    expect(tooltip).toContain('usageTrendHoverPoint');
    expect(tooltip).toContain('usageTrendTipOffset');
    expect(page).toContain('sm:grid-cols-3');
    expect(page).not.toContain('lg:grid-cols-5');
    expect(tooltip).toContain('dailyTotal');
    expect(tooltip).toContain('cumulativeTotal');
  });

  it('marks Cursor overview cards as not managing authorization', () => {
    const overview = source('AgentOverview.tsx');
    const model = source('agentOverviewModel.ts');
    expect(model).toContain("t('dashboard.overview.authUnsupported')");
    expect(model).toContain("kind: 'none'");
    expect(overview).toContain("view.action.kind !== 'none'");
    expect(translate('zh', 'dashboard.overview.authUnsupported')).toBe('不支持管理授权');
    expect(translate('zh', 'dashboard.overview.authUnsupported')).toBe(
      translate('zh', 'connections.capability.authUnsupported'),
    );
    expect(translate('en', 'dashboard.overview.authUnsupported')).toBe(
      translate('en', 'connections.capability.authUnsupported'),
    );
  });

  it('does not open a connect popup from overview cards or show quick actions', () => {
    const page = source('index.tsx');
    expect(page).not.toContain('onConnectRequest');
    expect(page).not.toContain("t('dashboard.page.quickActions')");
    expect(page).not.toContain('openForAgentConnect');
    expect(page).not.toContain('handleBackupNow');
    expect(page).not.toContain('createBackup');
    expect(page).not.toContain('handleBackupAll');
    expect(page).toContain('<ConnectFlowDialog');
  });

  it('uses the same connection-state words as Connections', () => {
    const page = source('index.tsx');
    expect(page).toContain('dashboardBindingMeta');
    expect(page).not.toContain('connectionStateRouteLabel');
    expect(translate('zh', 'dashboard.overview.hintAccount')).toBe(translate('zh', 'kind.oauth'));
    expect(translate('zh', 'dashboard.overview.hintApi')).toBe(translate('zh', 'kind.apikey'));
    expect(translate('en', 'dashboard.overview.hintAccount')).toBe(translate('en', 'kind.oauth'));
    expect(translate('en', 'dashboard.overview.hintApi')).toBe(translate('en', 'kind.apikey'));
  });

  it('does not show 本机路由 chrome on overview cards', () => {
    const overview = source('AgentOverview.tsx');
    const page = source('index.tsx');
    expect(overview).not.toContain('viaCompatible');
    expect(overview).not.toContain('manageLocalRoute');
    expect(overview).not.toContain('bridgesHrefForProfile');
    expect(overview).not.toContain('view.bridge');
    expect(overview).not.toContain('viaAdapter');
    expect(page).not.toContain('listAdapterProfiles');
    expect(page).not.toContain('getAdapterBridgeStatus');
    expect(page).not.toContain('viaAdapter');
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

describe('dashboard user-facing copy', () => {
  it('keeps dashboard copy free of banned jargon', () => {
    const keys = flattenKeys(zh).filter((key) => key.startsWith('dashboard.'));
    expect(keys.length).toBeGreaterThan(20);
    for (const key of keys) {
      expect(lookup(zh, key), key).not.toMatch(BANNED_UI);
      expect(lookup(en, key), key).not.toMatch(BANNED_UI);
    }
  });
});
