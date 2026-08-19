import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  batchAdoptToast,
  batchEnableToast,
  catalogFilters,
  marketSuffix,
  privateSkillRowHint,
  sharedRootPresence,
  skillCellTip,
} from './copy';

describe('skills copy via createTranslator(zh)', () => {
  const t = createTranslator('zh');

  it('cell tips stay short and avoid L3 jargon', () => {
    const absent = skillCellTip(t, 'Claude', 'absent', 'available');
    const linked = skillCellTip(t, 'Claude', 'linked', 'available', 'junction');
    const conflict = skillCellTip(t, 'Codex', 'foreign', 'conflict');

    expect(absent).toBe('未启用 · 点击启用');
    expect(linked).toContain('已启用');
    expect(conflict).toContain('覆盖');

    for (const s of [absent, linked, conflict]) {
      expect(s).not.toMatch(/单向投影/);
      expect(s).not.toMatch(/非双向/);
      expect(s.length).toBeLessThan(40);
    }
  });

  it('toast titles stay compact', () => {
    expect(t('skills.toast.installOk').length).toBeLessThanOrEqual(16);
    expect(t('skills.toast.marketInstallOk').length).toBeLessThanOrEqual(16);
    expect(t('skills.toast.enableFailed')).toBe('无法启用');
    expect(t('skills.toast.disableFailed')).toBe('无法取消启用');
  });

  it('market meta is one short line', () => {
    const line = marketSuffix(t, true);
    expect(line.split('\n')).toHaveLength(1);
    expect(line).not.toMatch(/单向投影/);
    expect(line).not.toMatch(/git clone/);
  });

  it('local filters include private-only and projection conflict', () => {
    expect(t('skills.filters.enablePrivate')).toBe('只在本工具');
    expect(t('skills.filters.enableConflict')).toBe('冲突');
    expect(t('skills.filters.enableMapped')).toBe('已启用');
    expect(t('skills.filters.enableUnmapped')).toBe('未启用');
  });

  it('private-source instructional copy is a hover hint, not list chrome', () => {
    expect(skillCellTip(t, 'Claude', 'absent', 'private_source')).toBe(
      privateSkillRowHint(t),
    );
    expect(privateSkillRowHint(t)).toBe('只在本工具 · 先加入共享库');
    expect(privateSkillRowHint(t)).toContain('先加入共享库');
  });

  it('tabs are library + market only', () => {
    const tabKeys = ['library', 'market', 'libraryBadge', 'privateBadge'];
    expect(tabKeys).not.toContain('workspace');
    expect(t('skills.tabs.library')).toBe('本地技能');
    expect(t('skills.tabs.market')).toBe('技能市场');
    expect(t('skills.tabs.libraryBadge', { n: 12 })).toBe('12 个本地技能');
    expect(t('skills.menu.removePrivate')).toBe('从该工具目录删除');
  });

  it('shared-root column names the path and states presence', () => {
    const root = '~/.agents/skills';
    expect(t('skills.matrix.sharedRoot')).toBe(root);
    expect(sharedRootPresence(t, true, root)).toBe(`已在 ${root}`);
    expect(sharedRootPresence(t, false, root)).toBe(`未加入 ${root}`);
  });

  it('empty-library title differs from filter-miss title', () => {
    expect(t('skills.empty.emptyLibraryTitle')).toBe('还没有技能');
    expect(t('skills.empty.noMatchTitle')).toBe('没有匹配的技能');
  });

  it('adopt toast stays on the local table', () => {
    expect(t('skills.toast.adoptOkDesc')).toBe('已可在矩阵中启用');
    expect(batchAdoptToast(t, 1, 0, 0)).not.toHaveProperty('actionLabel');
    expect(batchEnableToast(t, 1, 0, []).title).toBe('已启用所选 1 项');
  });

  it('catalog filter ids stay library-only', () => {
    const filters = catalogFilters(t);
    expect(filters.map((f) => f.id)).toEqual(['all', 'private', 'mapped', 'unmapped', 'conflict']);
    expect(filters.find((f) => f.id === 'private')?.label).toBe('只在本工具');
  });

  it('splits first-run empty library title from filter-miss title', () => {
    expect(t('skills.empty.emptyLibraryTitle')).toBe('还没有技能');
    expect(t('skills.empty.noMatchTitle')).toBe('没有匹配的技能');
  });
});
