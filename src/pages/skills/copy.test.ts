import { describe, expect, it } from 'vitest';
import { skillsCopy } from './copy';

describe('skillsCopy', () => {
  it('cell tips stay short and avoid L3 jargon', () => {
    const absent = skillsCopy.cell.tip('Claude', 'absent', 'available');
    const linked = skillsCopy.cell.tip('Claude', 'linked', 'available', 'junction');
    const conflict = skillsCopy.cell.tip('Codex', 'foreign', 'conflict');

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
    expect(skillsCopy.toast.installOk.title.length).toBeLessThanOrEqual(16);
    expect(skillsCopy.toast.marketInstallOk('pdf').title.length).toBeLessThanOrEqual(16);
    expect(skillsCopy.toast.enableFailed('x').title).toBe('无法启用');
    expect(skillsCopy.toast.disableFailed('x').title).toBe('无法取消启用');
  });

  it('market meta is one short line', () => {
    const line = skillsCopy.market.suffix(true);
    expect(line.split('\n')).toHaveLength(1);
    expect(line).not.toMatch(/单向投影/);
    expect(line).not.toMatch(/git clone/);
  });
});
