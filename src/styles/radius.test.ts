/**
 * 圆角契约：生产源码只用语义 token，禁止魔法值和禁用 Tailwind 名。
 * 真源见 `RADIUS` / docs/ui-design.md §2。
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

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

/** 禁用的 Tailwind 圆角名 + 任意魔法值。允许 btn/card/composer/full/mark/none。 */
const FORBIDDEN_RADIUS =
  /\brounded-(?:sm|md|lg|xl|2xl|3xl|xs)\b|rounded-\[[^\]]+\]/;

function isCommentLine(line: string): boolean {
  const t = line.trim();
  return t.startsWith('//') || t.startsWith('*') || t.startsWith('/*') || t.startsWith('·');
}

describe('radius source contract', () => {
  it('does not use magic or alias radius classes in production source', () => {
    const hits: string[] = [];
    for (const abs of walkSourceFiles(srcRoot)) {
      const rel = toPosixRel(abs);
      if (rel === 'styles/tokens.ts') continue;
      const src = readFileSync(abs, 'utf8');
      for (const [i, line] of src.split('\n').entries()) {
        if (isCommentLine(line)) continue;
        if (FORBIDDEN_RADIUS.test(line)) hits.push(`${rel}:${i + 1}`);
      }
    }
    expect(hits).toEqual([]);
  });

  it('tailwind radius scale has only semantic keys', () => {
    const cfg = readFileSync(path.join(srcRoot, '..', 'tailwind.config.ts'), 'utf8');
    expect(cfg).toContain("btn: 'var(--radius-sm)'");
    expect(cfg).toContain("card: 'var(--radius)'");
    expect(cfg).toContain("composer: 'var(--radius-lg)'");
    expect(cfg).toContain("mark: 'var(--radius-mark)'");
    expect(cfg).not.toContain("sm: 'var(--radius-sm)'");
    expect(cfg).not.toContain("lg: 'var(--radius-lg)'");
  });
});
