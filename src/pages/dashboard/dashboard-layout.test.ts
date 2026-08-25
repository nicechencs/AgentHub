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

  it('does not repeat Agent 总览 or a Manage button above the cards', () => {
    const overview = source('AgentOverview.tsx');
    expect(overview).not.toContain("t('dashboard.overview.title')");
    expect(overview).not.toContain("t('dashboard.overview.manage')");
    expect(overview).not.toContain("from '@/components/ui/button'");
    expect(source('index.tsx')).not.toContain("t('dashboard.overview.manage')");
  });

  it('wires 立即备份 to Settings backups instead of createBackup on the first agent', () => {
    const page = source('index.tsx');
    expect(page).toContain('dashboardBackupNowHref');
    expect(page).toContain('handleBackupNow');
    expect(page).not.toContain('createBackup');
    expect(page).not.toContain('handleBackupAll');
    expect(source('dashboard-actions.ts')).toContain("settingsSearch('backups')");
  });
});
