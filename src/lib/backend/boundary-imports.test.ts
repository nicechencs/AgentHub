import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function sourceOf(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('pages/hooks do not import backend/tauri', () => {
  it('agent-card uses the install façade, not tauri install-events', () => {
    const src = sourceOf('pages/agents/agent-card.tsx');
    expect(src).not.toMatch(/@\/lib\/backend\/tauri/);
    expect(src).not.toMatch(/isTauriApp/);
    expect(src).toMatch(/from '@\/lib\/api\/install'/);
  });

  it('useSkills uses the skill façade, not tauri skill-events', () => {
    const src = sourceOf('lib/hooks/useSkills.ts');
    expect(src).not.toMatch(/@\/lib\/backend\/tauri/);
    expect(src).not.toMatch(/isTauriApp/);
    expect(src).toMatch(/onSkillsFsChanged/);
    expect(src).toMatch(/from '@\/lib\/api\/skill'/);
  });

  it('tauri update uses the shared invoke wrapper for controlled restart', () => {
    const src = sourceOf('lib/backend/tauri/update.ts');
    expect(src).not.toMatch(/from '@tauri-apps\/api\/core'/);
    expect(src).toMatch(/from '\.\/invoke'/);
    expect(src).toMatch(/invoke\('request_controlled_restart'\)/);
  });
});
