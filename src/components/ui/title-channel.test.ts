/**
 * Hint 通道契约：业务侧不得用原生 title 当教学/截断提示。
 * Button / Input / AgentDot 的 title prop 会转成 Hint，不在本扫描内。
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function walkSourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const full = path.join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walkSourceFiles(full, out);
      continue;
    }
    if (/\.(ts|tsx)$/.test(name) && !/\.(test|spec)\.(ts|tsx)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

function toPosixRel(abs: string): string {
  return path.relative(srcRoot, abs).split(path.sep).join('/');
}

/** Native title on text / raw-button nodes (not Button/Input title→Hint). */
const NATIVE_TITLE_ON_TEXT =
  /<(p|span|div|h[1-6]|li|label|pre|button)\b[^>]*\stitle\s*=/;

describe('tooltip channel (docs/ui-experience-alignment.md §5.2)', () => {
  it('pages and layout do not put native title on text or raw button nodes', () => {
    const roots = ['pages', 'components/layout', 'components/shared'].map((rel) =>
      path.join(srcRoot, rel),
    );
    const hits: string[] = [];
    for (const root of roots) {
      for (const abs of walkSourceFiles(root)) {
        const rel = toPosixRel(abs);
        const src = readFileSync(abs, 'utf8');
        if (NATIVE_TITLE_ON_TEXT.test(src)) hits.push(rel);
      }
    }
    expect(hits).toEqual([]);
  });

  it('AgentTabStrip count hint is merged into the tab Hint, not a native title', () => {
    const src = readFileSync(path.join(srcRoot, 'components/layout/AgentTabStrip.tsx'), 'utf8');
    expect(src).not.toContain('title={countTitle');
    expect(src).toContain('countHint');
  });
});
