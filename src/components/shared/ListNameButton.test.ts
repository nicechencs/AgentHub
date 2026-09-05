import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { LIST_NAME_BUTTON_CLASS } from './ListNameButton';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('ListNameButton contract', () => {
  it('is a compact name control, not a padded Button', () => {
    expect(LIST_NAME_BUTTON_CLASS).toContain('text-body');
    expect(LIST_NAME_BUTTON_CLASS).toContain('font-medium');
    expect(LIST_NAME_BUTTON_CLASS).toContain('text-primary');
    expect(LIST_NAME_BUTTON_CLASS).toContain('hover:underline');
    expect(LIST_NAME_BUTTON_CLASS).toContain('focus-visible:ring-2');
    expect(source('components/shared/ListNameButton.tsx')).not.toContain("from '@/components/ui/button'");
  });

  it('is the inspect name on dense field tables', () => {
    expect(source('pages/agents/agent-card.tsx')).toContain('<ListNameButton');
    expect(source('pages/agents/agent-card.tsx')).toContain('data-agent-name');
    expect(source('pages/connections/TicketWalletList.tsx')).toContain('<ListNameButton');
    expect(source('pages/connections/TicketWalletList.tsx')).toContain('data-ticket-name');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).toContain('<ListNameButton');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).toContain('data-pool-login-name');
    expect(source('pages/skills/SkillMatrix.tsx')).toContain('<ListNameButton');
    expect(source('pages/skills/SkillsProjectPanel.tsx')).toContain('<ListNameButton');
    expect(source('pages/projects/ProjectSessionRow.tsx')).toContain('<ListNameButton');
  });

  it('opens inspect from the name; row onOpen only follows an already-open pane', () => {
    expect(source('pages/agents/agent-card.tsx')).not.toContain('onOpen={onSelect}');
    expect(source('pages/agents/agent-card.tsx')).toContain('onOpen={onFollow}');
    expect(source('pages/connections/TicketWalletList.tsx')).not.toContain('onOpen={onShowDetail');
    expect(source('pages/connections/TicketWalletList.tsx')).toContain('onOpen={onFollowDetail');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).toContain('onOpen={onFollowDetail');
    expect(source('pages/routes/pool/PoolAuthorizationList.tsx')).not.toContain('onOpen={onShowDetail');
  });
});
