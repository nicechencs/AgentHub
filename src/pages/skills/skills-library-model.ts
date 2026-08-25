import type { InstalledSkillDto } from '@/lib/api/skill';
import {
  catalogRowHasConflict,
  catalogRowHasMapped,
  isPrivateSourceRow,
  isSharedCatalogRow,
} from './SkillMatrix';
import type { LocalFilter } from './skills-preview-model';

/** 筛选角标：全量计数，不受搜索影响 */
export function countLibraryFilters(
  localRows: InstalledSkillDto[],
): Record<LocalFilter, number> {
  let mapped = 0;
  let unmapped = 0;
  let conflict = 0;
  let privateRows = 0;
  for (const row of localRows) {
    if (isPrivateSourceRow(row) || row.origin !== 'shared') {
      privateRows++;
      continue;
    }
    if (catalogRowHasMapped(row)) mapped++;
    else unmapped++;
    if (catalogRowHasConflict(row)) conflict++;
  }
  return {
    all: localRows.length,
    private: privateRows,
    mapped,
    unmapped,
    conflict,
  };
}

export function filterLibraryRows(
  localRows: InstalledSkillDto[],
  search: string,
  filter: LocalFilter,
): InstalledSkillDto[] {
  const keyword = search.trim().toLowerCase();
  return localRows.filter((row) => {
    if (keyword && !row.name.toLowerCase().includes(keyword)) return false;
    if (filter === 'all') return true;
    if (filter === 'private') return row.origin !== 'shared';
    if (!isSharedCatalogRow(row)) return false;
    if (filter === 'mapped') return catalogRowHasMapped(row);
    if (filter === 'unmapped') return !catalogRowHasMapped(row);
    if (filter === 'conflict') return catalogRowHasConflict(row);
    return true;
  });
}

export function filteredSharedRows(filtered: InstalledSkillDto[]): InstalledSkillDto[] {
  return filtered.filter(isSharedCatalogRow);
}

export function allFilteredSharedSelected(
  filteredShared: InstalledSkillDto[],
  selected: ReadonlySet<string>,
): boolean {
  return filteredShared.length > 0 && filteredShared.every((s) => selected.has(s.id));
}

export function toggleSelectedSkill(
  selected: ReadonlySet<string>,
  skillId: string,
): Set<string> {
  const next = new Set(selected);
  if (next.has(skillId)) next.delete(skillId);
  else next.add(skillId);
  return next;
}

export function nextSelectedForToggleAll(
  filteredShared: InstalledSkillDto[],
  allSelected: boolean,
): Set<string> {
  return allSelected ? new Set() : new Set(filteredShared.map((s) => s.id));
}
