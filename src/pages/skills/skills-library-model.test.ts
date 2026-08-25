import { describe, expect, it } from 'vitest';
import type { InstalledSkillDto } from '@/lib/api/skill';
import {
  allFilteredSharedSelected,
  countLibraryFilters,
  filterLibraryRows,
  filteredSharedRows,
  nextSelectedForToggleAll,
  toggleSelectedSkill,
} from './skills-library-model';

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

const mappedShared = row({
  id: 'pdf',
  name: 'PDF helper',
  origin: 'shared',
  projections: [{ agent: 'claude', state: 'copied' }],
});

const unmappedShared = row({
  id: 'csv',
  name: 'CSV tools',
  origin: 'shared',
  projections: [{ agent: 'claude', state: 'absent' }],
});

const mappedConflict = row({
  id: 'xlsx',
  name: 'Excel pack',
  origin: 'shared',
  projections: [{ agent: 'claude', state: 'linked', mapStatus: 'conflict' }],
});

const unmappedConflict = row({
  id: 'md',
  name: 'Markdown lint',
  origin: 'shared',
  projections: [{ agent: 'cursor', state: 'absent', mapStatus: 'conflict' }],
});

const privateSource = row({
  id: 'pet',
  name: 'Pet skill',
  origin: 'codex',
  mapStatus: 'private_source',
});

/** origin !== 'shared' but not isPrivateSourceRow — still private in counts/filter. */
const nonSharedProjection = row({
  id: 'ghost',
  name: 'Ghost copy',
  origin: 'claude',
  mapStatus: 'available',
  projections: [{ agent: 'claude', state: 'copied', mapStatus: 'conflict' }],
});

const localRows = [
  mappedShared,
  unmappedShared,
  mappedConflict,
  unmappedConflict,
  privateSource,
  nonSharedProjection,
];

function ids(rows: InstalledSkillDto[]): string[] {
  return rows.map((item) => item.id);
}

describe('countLibraryFilters', () => {
  it('splits shared mapped/unmapped/conflict from private rows', () => {
    expect(countLibraryFilters(localRows)).toEqual({
      all: 6,
      private: 2,
      mapped: 2,
      unmapped: 2,
      conflict: 2,
    });
  });

  it('does not let search change counts (counts take unfiltered localRows)', () => {
    const searched = filterLibraryRows(localRows, 'pdf', 'all');
    expect(ids(searched)).toEqual(['pdf']);
    expect(countLibraryFilters(localRows)).toEqual({
      all: 6,
      private: 2,
      mapped: 2,
      unmapped: 2,
      conflict: 2,
    });
    expect(countLibraryFilters(searched)).not.toEqual(countLibraryFilters(localRows));
  });
});

describe('filterLibraryRows', () => {
  it('matches name keyword case-insensitively after trim', () => {
    expect(ids(filterLibraryRows(localRows, '  PDF  ', 'all'))).toEqual(['pdf']);
  });

  it('keeps every row when filter is all and search is empty', () => {
    expect(ids(filterLibraryRows(localRows, '   ', 'all'))).toEqual(ids(localRows));
  });

  it('treats private as origin !== shared', () => {
    expect(ids(filterLibraryRows(localRows, '', 'private'))).toEqual(['pet', 'ghost']);
  });

  it('keeps shared mapped rows including mapped conflicts', () => {
    expect(ids(filterLibraryRows(localRows, '', 'mapped'))).toEqual(['pdf', 'xlsx']);
  });

  it('keeps shared unmapped rows including unmapped conflicts', () => {
    expect(ids(filterLibraryRows(localRows, '', 'unmapped'))).toEqual(['csv', 'md']);
  });

  it('keeps shared conflict rows whether or not they are mapped', () => {
    expect(ids(filterLibraryRows(localRows, '', 'conflict'))).toEqual(['xlsx', 'md']);
  });

  it('applies keyword on top of a mapped/unmapped/conflict filter', () => {
    expect(ids(filterLibraryRows(localRows, 'excel', 'mapped'))).toEqual(['xlsx']);
    expect(ids(filterLibraryRows(localRows, 'csv', 'unmapped'))).toEqual(['csv']);
    expect(ids(filterLibraryRows(localRows, 'lint', 'conflict'))).toEqual(['md']);
  });

  it('drops private rows from mapped/unmapped/conflict even if projections look mapped', () => {
    expect(ids(filterLibraryRows(localRows, '', 'mapped'))).not.toContain('ghost');
    expect(ids(filterLibraryRows(localRows, '', 'conflict'))).not.toContain('ghost');
  });
});

describe('selection helpers', () => {
  const sharedOnly = filteredSharedRows(localRows);

  it('keeps only shared catalog rows', () => {
    expect(ids(sharedOnly)).toEqual(['pdf', 'csv', 'xlsx', 'md']);
  });

  it('toggles one id without mutating the previous set', () => {
    const prev = new Set(['pdf']);
    const added = toggleSelectedSkill(prev, 'csv');
    expect([...added].sort()).toEqual(['csv', 'pdf']);
    expect([...prev]).toEqual(['pdf']);
    const removed = toggleSelectedSkill(added, 'pdf');
    expect([...removed]).toEqual(['csv']);
    expect([...added].sort()).toEqual(['csv', 'pdf']);
  });

  it('reports all-selected only when every filtered shared id is present', () => {
    expect(allFilteredSharedSelected([], new Set())).toBe(false);
    expect(allFilteredSharedSelected(sharedOnly, new Set(['pdf', 'csv']))).toBe(false);
    expect(
      allFilteredSharedSelected(sharedOnly, new Set(['pdf', 'csv', 'xlsx', 'md'])),
    ).toBe(true);
  });

  it('select-all replaces the set with filtered shared ids; clear-all empties it', () => {
    expect([...nextSelectedForToggleAll(sharedOnly, false)].sort()).toEqual([
      'csv',
      'md',
      'pdf',
      'xlsx',
    ]);
    expect([...nextSelectedForToggleAll(sharedOnly, true)]).toEqual([]);
  });
});
