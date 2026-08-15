import { describe, expect, it } from 'vitest';
import type { InstalledSkillDto } from '@/lib/api/skill';
import {
  isPrivateSourceRow,
  isSharedCatalogRow,
  privateRowOriginId,
  sharedRootColumnLabel,
  visibleCatalogRows,
} from './SkillMatrix';

function row(
  partial: Partial<InstalledSkillDto> & Pick<InstalledSkillDto, 'id' | 'origin'>,
): InstalledSkillDto {
  return {
    name: partial.id,
    description: '',
    sourceDir: '',
    rootLabel: '',
    rootDir: '',
    projectable: partial.origin === 'shared',
    mapStatus: partial.origin === 'shared' ? 'available' : 'private_source',
    source: null,
    projections: [],
    ...partial,
  };
}

describe('sharedRootColumnLabel', () => {
  it('falls back to ~/.agents/skills when no shared rootLabel exists', () => {
    expect(sharedRootColumnLabel([])).toBe('~/.agents/skills');
    expect(
      sharedRootColumnLabel([
        row({ id: 'pet', origin: 'codex', rootLabel: '~/.codex/skills' }),
      ]),
    ).toBe('~/.agents/skills');
  });

  it('uses the shared catalog rootLabel', () => {
    expect(
      sharedRootColumnLabel([
        row({ id: 'pet', origin: 'codex', rootLabel: '~/.codex/skills' }),
        row({ id: 'pdf', origin: 'shared', rootLabel: '  ~/.agents/skills  ' }),
      ]),
    ).toBe('~/.agents/skills');
  });
});

describe('local tab count', () => {
  it('counts shared + private-only visible rows', () => {
    const rows = [
      row({ id: 'pdf', origin: 'shared', rootLabel: '~/.agents/skills' }),
      row({ id: 'pet', origin: 'cursor', mapStatus: 'private_source' }),
      row({
        id: 'pdf',
        origin: 'claude',
        mapStatus: 'available',
        rootLabel: '~/.claude/skills',
      }),
    ];
    const visible = visibleCatalogRows(rows);
    expect(visible).toHaveLength(2);
    expect(visible.filter(isSharedCatalogRow)).toHaveLength(1);
    expect(visible.filter(isPrivateSourceRow)).toHaveLength(1);
  });
});

describe('private row column placement', () => {
  it('keeps Cursor private skills on the cursor column, not claude', () => {
    const cursorPrivate = row({
      id: 'hatch-pet',
      origin: 'cursor',
      mapStatus: 'private_source',
      rootLabel: '~/.cursor/skills-cursor',
    });
    const claudePrivate = row({
      id: 'local-review',
      origin: 'claude',
      mapStatus: 'private_source',
      rootLabel: '~/.claude/skills',
    });
    expect(privateRowOriginId(cursorPrivate)).toBe('cursor');
    expect(privateRowOriginId(cursorPrivate)).not.toBe('claude');
    expect(privateRowOriginId(claudePrivate)).toBe('claude');
    expect(privateRowOriginId(row({ id: 'pdf', origin: 'shared' }))).toBeNull();
  });
});

describe('shared-root presence', () => {
  it('treats shared-origin rows as in ~/.agents/skills', () => {
    const shared = row({
      id: 'pdf',
      origin: 'shared',
      rootLabel: '~/.agents/skills',
    });
    const priv = row({
      id: 'pet',
      origin: 'codex',
      mapStatus: 'private_source',
      rootLabel: '~/.codex/skills',
    });
    expect(isSharedCatalogRow(shared)).toBe(true);
    expect(isSharedCatalogRow(priv)).toBe(false);
    expect(isPrivateSourceRow(priv)).toBe(true);
  });
});
