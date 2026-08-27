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

  it('plots overlay usage series in agent brand hex, not a stacked CSS-var area', () => {
    const page = source('index.tsx');
    expect(page).toContain('resolveChartColor');
    expect(page).toContain('type="monotone"');
    expect(page).toContain('isAnimationActive={false}');
    expect(page).not.toContain('stackId');
    expect(page).not.toContain('type="linear"');
    expect(page).not.toContain('stroke={meta.color}');
    expect(page).not.toContain('stopColor={meta.color}');
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
    expect(page).toContain('connectionStateRouteLabel');
    expect(page).toContain("from '@/pages/connections/ticket-wallet-model'");
    expect(translate('zh', 'dashboard.overview.hintAccount')).toBe(translate('zh', 'kind.oauth'));
    expect(translate('zh', 'dashboard.overview.hintApi')).toBe(translate('zh', 'kind.apikey'));
    expect(translate('zh', 'dashboard.overview.viaCompatible')).toBe(
      translate('zh', 'kind.route.localRoute'),
    );
    expect(translate('en', 'dashboard.overview.hintAccount')).toBe(translate('en', 'kind.oauth'));
    expect(translate('en', 'dashboard.overview.hintApi')).toBe(translate('en', 'kind.apikey'));
    expect(translate('en', 'dashboard.overview.viaCompatible')).toBe(
      translate('en', 'kind.route.localRoute'),
    );
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
