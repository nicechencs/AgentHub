import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

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
    expect(overview).toContain('useAgentCatalogOptional');
    expect(overview).toContain('catalog.hydrated');
  });

  it('does not repeat Agent 总览 or a Manage button above the cards', () => {
    const overview = source('AgentOverview.tsx');
    expect(overview).not.toContain("t('dashboard.overview.title')");
    expect(overview).not.toContain("t('dashboard.overview.manage')");
    expect(overview).not.toContain("from '@/components/ui/button'");
    expect(source('index.tsx')).not.toContain("t('dashboard.overview.manage')");
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
});
