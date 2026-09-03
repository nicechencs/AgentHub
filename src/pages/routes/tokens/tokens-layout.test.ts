import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('routes tokens layout wiring', () => {
  it('creates entry keys from four endpoint cards, not a type dropdown', () => {
    const page = source('index.tsx');
    expect(page).toContain('CreateTokenEndpointCards');
    expect(page).toContain('buildCreateTokenEndpointCards');
    expect(page).toContain('firstCreateTokenPoolId');
    expect(page).toContain('defaultCreateTokenName');
    expect(page).toContain('createNamePlaceholder');
    expect(page).not.toContain('SelectTrigger');
    expect(page).not.toContain('SelectItem');
    expect(page).not.toContain("from '@/components/ui/select'");
  });

  it('shows one endpoint row with supported Agent logos under the path', () => {
    const cards = source('CreateTokenEndpointCards.tsx');
    expect(cards).toContain('role="radiogroup"');
    expect(cards).toContain('role="radio"');
    expect(cards).toContain('data-create-endpoint');
    expect(cards).toContain('AgentLogo');
    expect(cards).toContain('flex flex-col gap-2');
    expect(cards).not.toContain('grid-cols');
  });

  it('docks detail actions in the inspect header like the connection pool', () => {
    const panel = source('TokenDetailPanel.tsx');
    const page = source('index.tsx');
    expect(panel).toContain('headerActions=');
    expect(panel).toContain('data-token-test');
    expect(panel).toContain('data-token-delete');
    expect(panel).toContain('data-token-edit-key');
    expect(panel).toContain('TokenImportToAgentButton');
    expect(panel).toContain('<Trash2');
    expect(page).toContain('onDelete={() => setDeleteRow(detailRow)}');
    expect(page).not.toContain('onDelete={detailRow.canDelete');
  });

  it('puts eye and copy icons after the entry key, not text buttons', () => {
    const panel = source('TokenDetailPanel.tsx');
    expect(panel).toContain('<Eye');
    expect(panel).toContain('<EyeOff');
    expect(panel).toContain('<Copy');
    expect(panel).toContain('data-token-reveal');
    expect(panel).toContain('data-token-copy');
    expect(panel).toContain('flex min-w-0 items-center gap-1');
    expect(panel).not.toContain(">{revealed ? t('common.hideSecret') : t('common.showSecret')}</Button>");
    expect(panel).not.toContain(">{t('routes.tokens.copy')}</Button>");
  });
});
